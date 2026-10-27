//! String-level composition: `<page src>`, `<layout src>` and `<include src>`.
//!
//! Improvements over the original:
//! * includes may contain includes (with cycle detection),
//! * layouts may themselves use a layout (nested layouts),
//! * pages without a `<layout>` still get their includes resolved,
//! * `<include src="x.html" />` self-closing form is accepted,
//! * missing files produce errors that name the file that referenced them.

use anyhow::{Context, Result, bail};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use super::tags::{find_tag_from, line_of, scan_tag};

const MAX_DEPTH: usize = 32;

// Composition splices files into one another, so a line number in the composed
// page says nothing about where its text came from. Every spliced piece starts
// with a marker naming its file and line, and the text after it resumes with
// another. `locate` reads them back for error messages; the template parser
// strips them, so they never reach the output. Private-use characters keep
// them out of anything a user would write.
const MARK: char = '\u{E000}';
const MARK_SEP: char = '\u{E001}';
const MARK_END: char = '\u{E002}';

fn mark(file: &str, line: usize) -> String {
    format!("{MARK}{file}{MARK_SEP}{line}{MARK_END}")
}

/// A marker for the position `offset` in `src`, so text spliced in there
/// keeps pointing at where it came from. Empty when `src` carries no markers.
fn mark_at(src: &str, offset: usize) -> String {
    locate(src, offset)
        .map(|(f, l)| mark(f, l))
        .unwrap_or_default()
}

/// The file and line that byte `offset` of a composed page came from.
pub fn locate(src: &str, offset: usize) -> Option<(&str, usize)> {
    let before = &src[..offset];
    let start = before.rfind(MARK)?;
    let rest = &before[start + MARK.len_utf8()..];
    let end = rest.find(MARK_END)?;
    let (file, line) = rest[..end].split_once(MARK_SEP)?;
    let newlines = rest[end..].matches('\n').count();
    Some((file, line.parse::<usize>().ok()? + newlines))
}

/// `file:line` for an error at `offset`, falling back to `origin` and the
/// line within `src` when it carries no markers.
pub fn at(src: &str, offset: usize, origin: &str) -> String {
    match locate(src, offset) {
        Some((file, line)) => format!("{file}:{line}"),
        None => format!("{origin}:{}", line_of(src, offset)),
    }
}

/// `s` without composition markers.
pub fn strip_marks(s: &str) -> Cow<'_, str> {
    if !s.contains(MARK) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(MARK) {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find(MARK_END) {
            Some(e) => &rest[i + e + MARK_END.len_utf8()..],
            None => "",
        };
    }
    out.push_str(rest);
    Cow::Owned(out)
}

pub struct Composer<'a> {
    pub root: &'a Path,
}

impl Composer<'_> {
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(self.root)
            .unwrap_or(p)
            .display()
            .to_string()
            .replace('\\', "/")
    }

    fn read(&self, path: &Path, referenced_from: &str) -> Result<String> {
        std::fs::read_to_string(path)
            .with_context(|| format!("{referenced_from}: cannot read `{}`", self.rel(path)))
    }

    /// Load a page file and apply `<page>`, `<layout>` and `<include>`.
    pub fn compose_page(&self, path: &Path) -> Result<String> {
        Ok(strip_marks(&self.compose_page_with_attrs(path)?.0).into_owned())
    }

    /// Like [`compose_page`], also returning the page's own `<layout>`
    /// attributes (e.g. `title`), used for `{page.title}` and `{seo}`. The
    /// page still carries origin markers for [`super::parse`] to read.
    pub fn compose_page_with_attrs(&self, path: &Path) -> Result<(String, Vec<(String, String)>)> {
        let origin = self.rel(path);
        let mut src =
            std::fs::read_to_string(path).with_context(|| format!("cannot read `{origin}`"))?;
        let mut file = origin.clone();

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
            let target = self.root.join("pages").join(target.trim_start_matches('/'));
            src = self.read(&target, &origin)?;
            file = self.rel(&target);
        }
        let src = mark(&file, 1) + &src;

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
        let line = at(&src, open.start, origin);
        let Some(close_at) = src.rfind("</layout>") else {
            bail!("{line}: <layout> is never closed with </layout>");
        };
        if close_at < open.end {
            bail!("{line}: </layout> appears before <layout>");
        }
        let Some(layout_src) = open.attr("src") else {
            bail!("{line}: <layout> must include a src attribute, e.g. <layout src=\"main.html\">");
        };
        let layout_path = self
            .root
            .join("layouts")
            .join(layout_src.trim_start_matches('/'));
        let layout = self.read(&layout_path, &line)?;
        let layout_origin = self.rel(&layout_path);

        let layout = mark(&layout_origin, 1) + &layout;
        let layout = self.resolve_includes(&layout, &layout_origin, &mut Vec::new())?;
        let body = mark_at(&src, open.end) + &src[open.end..close_at];
        let body = self.resolve_includes(&body, origin, &mut Vec::new())?;

        let mut out = if let Some(i) = layout.find("{slot}") {
            let after = i + "{slot}".len();
            format!(
                "{}{body}{}{}",
                &layout[..i],
                mark_at(&layout, after),
                &layout[after..]
            )
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
        // Only pages being composed carry markers; markdown never does, and
        // must not gain any since its HTML goes straight into the output.
        let marked = src.contains(MARK);
        let mut out = String::with_capacity(src.len());
        let mut pos = 0;
        while let Some(tag) = find_tag_from(src, pos, &["include"]) {
            out.push_str(&src[pos..tag.start]);
            pos = tag.end;
            if tag.closing {
                continue; // stray </include>
            }
            let line = at(src, tag.start, origin);
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
                    "{line}: include cycle: {} -> {}",
                    chain.join(" -> "),
                    self.rel(&path)
                );
            }
            if stack.len() >= MAX_DEPTH {
                bail!("{line}: includes nest too deeply");
            }
            let mut content = self.read(&path, &line)?;
            for (k, v) in &tag.attrs {
                content = content.replace(&format!("{{{k}}}"), v);
            }
            let inc_origin = self.rel(&path);
            if marked {
                content = mark(&inc_origin, 1) + &content;
            }
            stack.push(path.clone());
            let resolved = self.resolve_includes_inner(&content, &inc_origin, stack, false);
            stack.pop();
            out.push_str(&resolved?);
            out.push_str(&mark_at(src, pos));
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
    fn locate_follows_splices() {
        let d = site();
        fs::write(d.path().join("includes/two.html"), "i1\ni2").unwrap();
        let p = d.path().join("pages/l.html");
        fs::write(
            &p,
            "<layout title=\"L\" src=\"base.html\">\np2\n<include src=\"two.html\" />\np4</layout>",
        )
        .unwrap();
        let (html, _) = Composer { root: d.path() }
            .compose_page_with_attrs(&p)
            .unwrap();
        let find = |needle: &str| locate(&html, html.find(needle).unwrap()).unwrap();
        assert_eq!(find("<html>"), ("layouts/base.html", 1));
        assert_eq!(find("p2"), ("pages/l.html", 2));
        assert_eq!(find("i2"), ("includes/two.html", 2));
        assert_eq!(find("p4"), ("pages/l.html", 4));
        assert_eq!(find("</html>"), ("layouts/base.html", 1));
        assert!(!strip_marks(&html).contains(MARK));
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
