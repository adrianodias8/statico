//! A small, quote-aware scanner for the handful of custom tags Static understands
//! (`<layout>`, `<include>`, `<page>`, `<ForEach>`, `<If>`, `<ElseIf>`, `<Else>`).
//!
//! Unlike the regexes in the original JavaScript implementation, this scanner
//! respects quoted attribute values, so `<If condition="loop > 1">` works.

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// Byte offset of the `<`.
    pub start: usize,
    /// Byte offset just past the `>`.
    pub end: usize,
    pub name: String,
    pub closing: bool,
    pub self_closing: bool,
    pub attrs: Vec<(String, String)>,
}

impl Tag {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Find every tag in `src` whose name is in `names`, in document order.
pub fn find_tags(src: &str, names: &[&str]) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(off) = src[pos..].find('<') {
        let start = pos + off;
        match scan_tag(src, start, names) {
            Some(tag) => {
                pos = tag.end;
                out.push(tag);
            }
            None => pos = start + 1,
        }
    }
    out
}

/// Find the first tag with one of `names` at or after `from`.
pub fn find_tag_from(src: &str, from: usize, names: &[&str]) -> Option<Tag> {
    let mut pos = from;
    while let Some(off) = src.get(pos..)?.find('<') {
        let start = pos + off;
        if let Some(tag) = scan_tag(src, start, names) {
            return Some(tag);
        }
        pos = start + 1;
    }
    None
}

/// Try to scan a tag starting at `start` (which must point at `<`).
pub fn scan_tag(src: &str, start: usize, names: &[&str]) -> Option<Tag> {
    let bytes = src.as_bytes();
    let mut i = start + 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let name_start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
    {
        i += 1;
    }
    let name = &src[name_start..i];
    if name.is_empty() || !names.contains(&name) {
        return None;
    }
    // The name must be followed by whitespace, `>` or `/`.
    match bytes.get(i) {
        Some(b) if b.is_ascii_whitespace() || *b == b'>' || *b == b'/' => {}
        _ => return None,
    }

    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        match bytes.get(i)? {
            b'>' => {
                i += 1;
                break;
            }
            b'/' if bytes.get(i + 1) == Some(&b'>') => {
                self_closing = true;
                i += 2;
                break;
            }
            b'/' => {
                i += 1;
                continue;
            }
            _ => {}
        }
        let key_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'>' | b'/' | b'"' | b'\'')
        {
            i += 1;
        }
        if i == key_start {
            // Stray quote or similar: skip it so we always make progress.
            i += 1;
            continue;
        }
        let key = src[key_start..i].to_string();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let value = match bytes.get(i)? {
                q @ (b'"' | b'\'') => {
                    let q = *q;
                    let vstart = i + 1;
                    let rel = src.as_bytes()[vstart..].iter().position(|b| *b == q)?;
                    i = vstart + rel + 1;
                    src[vstart..vstart + rel].to_string()
                }
                _ => {
                    let vstart = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    src[vstart..i].to_string()
                }
            };
            attrs.push((key, value));
        } else {
            attrs.push((key, String::new()));
        }
    }

    Some(Tag {
        start,
        end: i,
        name: name.to_string(),
        closing,
        self_closing,
        attrs,
    })
}

/// 1-based line number of a byte offset, for error messages.
pub fn line_of(src: &str, offset: usize) -> usize {
    src[..offset.min(src.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_quoted_gt() {
        let tags = find_tags(r#"a <If condition="loop > 1">x</If> b"#, &["If"]);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].attr("condition"), Some("loop > 1"));
        assert!(tags[1].closing);
    }

    #[test]
    fn ignores_similar_names() {
        assert!(find_tags("<Iframe> <includes>", &["If", "include"]).is_empty());
    }

    #[test]
    fn self_closing_and_single_quotes() {
        let t = find_tags("<include src='a.html' title=\"T\" />", &["include"]);
        assert_eq!(t[0].attr("src"), Some("a.html"));
        assert_eq!(t[0].attr("title"), Some("T"));
        assert!(t[0].self_closing);
    }
}
