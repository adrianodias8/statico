//! String-level composition: `<page src>`, `<layout src>` and `<include src>`.
//!
//! Improvements over the original:
//! * includes may contain includes (with cycle detection),
//! * layouts may themselves use a layout (nested layouts),
//! * pages without a `<layout>` still get their includes resolved,
//! * `<include src="x.html" />` self-closing form is accepted,
//! * missing files produce errors that name the file that referenced them.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use super::tags::{find_tag_from, line_of, scan_tag};

const MAX_DEPTH: usize = 32;

pub struct Composer<'a> {
    pub root: &'a Path,
}

impl Composer<'_> {
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(self.root).unwrap_or(p).display().to_string()
    }

    fn read(&self, path: &Path, referenced_from: &str) -> Result<String> {
        std::fs::read_to_string(path)
            .with_context(|| format!("{referenced_from}: cannot read `{}`", self.rel(path)))
    }

    /// Load a page file and apply `<page>`, `<layout>` and `<include>`.
    pub fn compose_page(&self, path: &Path) -> Result<String> {
        Ok(self.compose_page_with_attrs(path)?.0)
    }

    /// Like [`compose_page`], also returning the page's own `<layout>`
    /// attributes (e.g. `title`), used for `{page.title}` and `{seo}`.
    pub fn compose_page_with_attrs(&self, path: &Path) -> Result<(String, Vec<(String, String)>)> {
        let origin = self.rel(path);
        let mut src =
            std::fs::read_to_string(path).with_context(|| format!("cannot read `{origin}`"))?;

        // `<page src="other.html"></page>` reuses another page wholesale.
        let mut hops = 0;
        while let Some(tag) = find_tag_from(&src, 0, &["page"]) {
            let Some(target) = tag.attr("src").map(str::to_string) else {
                break;
            };
            hops += 1;
            if hops > MAX_DEPTH {
                bail!("{origin}: <page> references nest too deeply (cycle?)");
            }
            src = self.read(
                &self.root.join("pages").join(target.trim_start_matches('/')),
                &origin,
            )?;
        }

        let attrs = find_tag_from(&src, 0, &["layout"])
            .filter(|t| !t.closing)
            .map(|t| t.attrs)
            .unwrap_or_default();
        Ok((self.apply_layouts(src, &origin, 0)?, attrs))
    }

    fn apply_layouts(&self, src: String, origin: &str, depth: usize) -> Result<String> {
        if depth > MAX_DEPTH {
            bail!("{origin}: layouts nest too deeply (does a layout use itself?)");
        }
        let Some(open) = find_tag_from(&src, 0, &["layout"]).filter(|t| !t.closing) else {
            // No layout: still resolve includes.
            return self.resolve_includes(&src, origin, &mut Vec::new());
        };
        let line = line_of(&src, open.start);
        let Some(close_at) = src.rfind("</layout>") else {
            bail!("{origin}:{line}: <layout> is never closed with </layout>");
        };
        if close_at < open.end {
            bail!("{origin}:{line}: </layout> appears before <layout>");
        }
        let Some(layout_src) = open.attr("src") else {
            bail!(
                "{origin}:{line}: <layout> must include a src attribute, e.g. <layout src=\"main.html\">"
            );
        };
        let layout_path = self
            .root
            .join("layouts")
            .join(layout_src.trim_start_matches('/'));
        let layout = self.read(&layout_path, &format!("{origin}:{line}"))?;
        let layout_origin = self.rel(&layout_path);

        let layout = self.resolve_includes(&layout, &layout_origin, &mut Vec::new())?;
        let body = self.resolve_includes(&src[open.end..close_at], origin, &mut Vec::new())?;

        let mut out = if layout.contains("{slot}") {
            layout.replacen("{slot}", &body, 1)
        } else {
            crate::diag::warn(format!(
                "{layout_origin}: has no {{slot}}; the content of {origin} is dropped"
            ));
            layout
        };
        for (k, v) in &open.attrs {
            out = out.replace(&format!("{{{k}}}"), v);
        }

        // Nested layouts: the layout file itself may be wrapped in a <layout>.
        if find_tag_from(&out, 0, &["layout"]).is_some_and(|t| !t.closing) {
            return self.apply_layouts(out, &layout_origin, depth + 1);
        }
        Ok(out)
    }

    /// Replace every `<include src="...">` (recursively).
    pub fn resolve_includes(
        &self,
        src: &str,
        origin: &str,
        stack: &mut Vec<PathBuf>,
    ) -> Result<String> {
        self.resolve_includes_inner(src, origin, stack, false)
    }

    /// For markdown: prose may mention `<include src="x.html">` literally, so a
    /// missing file is a warning (tag left as-is) instead of an error.
    pub fn resolve_includes_lenient(&self, src: &str, origin: &str) -> Result<String> {
        self.resolve_includes_inner(src, origin, &mut Vec::new(), true)
    }

    fn resolve_includes_inner(
        &self,
        src: &str,
        origin: &str,
        stack: &mut Vec<PathBuf>,
        lenient: bool,
    ) -> Result<String> {
        if !src.contains("<include") {
            return Ok(src.to_string());
        }
        let mut out = String::with_capacity(src.len());
        let mut pos = 0;
        while let Some(tag) = find_tag_from(src, pos, &["include"]) {
            out.push_str(&src[pos..tag.start]);
            pos = tag.end;
            if tag.closing {
                continue; // stray </include>
            }
            let line = line_of(src, tag.start);
            // Consume a directly following `</include>` (whitespace allowed).
            if !tag.self_closing {
                let rest = &src[pos..];
                let trimmed = rest.trim_start();
                if trimmed.starts_with("</include") {
                    let skip = rest.len() - trimmed.len();
                    if let Some(close) = scan_tag(src, pos + skip, &["include"]) {
                        pos = close.end;
                    }
                }
            }
            let Some(file) = tag.attr("src") else {
                // No src: not ours (e.g. prose talking about <include></include>).
                out.push_str(&src[tag.start..pos]);
                continue;
            };
            let path = self
                .root
                .join("includes")
                .join(file.trim_start_matches('/'));
            if lenient && !path.is_file() {
                crate::diag::warn(format!(
                    "{origin}: includes/{file} not found; leaving the tag as-is"
                ));
                out.push_str(&src[tag.start..pos]);
                continue;
            }
            if stack.contains(&path) {
                let chain: Vec<String> = stack.iter().map(|p| self.rel(p)).collect();
                bail!(
                    "{origin}:{line}: include cycle: {} -> {}",
                    chain.join(" -> "),
                    self.rel(&path)
                );
            }
            if stack.len() >= MAX_DEPTH {
                bail!("{origin}:{line}: includes nest too deeply");
            }
            let mut content = self.read(&path, &format!("{origin}:{line}"))?;
            for (k, v) in &tag.attrs {
                content = content.replace(&format!("{{{k}}}"), v);
            }
            stack.push(path.clone());
            let resolved = self.resolve_includes_inner(&content, &self.rel(&path), stack, false);
            stack.pop();
            out.push_str(&resolved?);
        }
        out.push_str(&src[pos..]);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn site() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for sub in ["pages", "layouts", "includes"] {
            fs::create_dir_all(r.join(sub)).unwrap();
        }
        fs::write(
            r.join("layouts/base.html"),
            "<html><title>{title}</title>{slot}</html>",
        )
        .unwrap();
        fs::write(
            r.join("layouts/docs.html"),
            "<layout src=\"base.html\" title=\"{title}\"><nav/>{slot}</layout>",
        )
        .unwrap();
        fs::write(
            r.join("includes/a.html"),
            "A[<include src=\"b.html\" who=\"{who}\"></include>]",
        )
        .unwrap();
        fs::write(r.join("includes/b.html"), "B:{who}").unwrap();
        fs::write(
            r.join("includes/loop.html"),
            "<include src=\"loop.html\" />",
        )
        .unwrap();
        d
    }

    #[test]
    fn layout_include_nested() {
        let d = site();
        let p = d.path().join("pages/index.html");
        fs::write(&p, "<layout title=\"Hi\" src=\"base.html\"><include src=\"a.html\" who=\"me\"></include></layout>").unwrap();
        let c = Composer { root: d.path() };
        assert_eq!(
            c.compose_page(&p).unwrap(),
            "<html><title>Hi</title>A[B:me]</html>"
        );
    }

    #[test]
    fn nested_layouts() {
        let d = site();
        let p = d.path().join("pages/doc.html");
        fs::write(&p, "<layout title=\"Doc\" src=\"docs.html\">body</layout>").unwrap();
        let c = Composer { root: d.path() };
        assert_eq!(
            c.compose_page(&p).unwrap(),
            "<html><title>Doc</title><nav/>body</html>"
        );
    }

    #[test]
    fn no_layout_still_includes() {
        let d = site();
        let p = d.path().join("pages/raw.html");
        fs::write(&p, "x<include src=\"b.html\" who=\"y\"/>z").unwrap();
        let c = Composer { root: d.path() };
        assert_eq!(c.compose_page(&p).unwrap(), "xB:yz");
    }

    #[test]
    fn cycle_detected() {
        let d = site();
        let p = d.path().join("pages/c.html");
        fs::write(&p, "<include src=\"loop.html\"></include>").unwrap();
        let c = Composer { root: d.path() };
        let e = c.compose_page(&p).unwrap_err().to_string();
        assert!(e.contains("cycle"), "{e}");
    }

    #[test]
    fn missing_include_names_origin() {
        let d = site();
        let p = d.path().join("pages/m.html");
        fs::write(&p, "\n\n<include src=\"nope.html\"></include>").unwrap();
        let c = Composer { root: d.path() };
        let e = format!("{:#}", c.compose_page(&p).unwrap_err());
        assert!(
            e.contains("pages/m.html:3") && e.contains("includes/nope.html"),
            "{e}"
        );
    }
}
