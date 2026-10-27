//! Diagnostics: warnings are collected (for `--json`) and printed (for humans).
//! Errors carry `file:line: message` prefixes that [`parse`] turns back into
//! structured data for machine-readable output.

use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);
static WARNINGS: Mutex<Vec<Diagnostic>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Diagnostic {
    pub level: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// The page URL, for problems found in built output (`statico check`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
}

/// In `--json` mode human-readable chatter is suppressed.
pub fn set_quiet(q: bool) {
    QUIET.store(q, Ordering::Relaxed);
}

pub fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// Print to stdout unless in JSON mode.
#[macro_export]
macro_rules! say {
    ($($t:tt)*) => { if !$crate::diag::quiet() { println!($($t)*); } };
}

pub fn warn(msg: impl Into<String>) {
    push_warning(msg.into(), None);
}

/// [`warn`] with a stable code that `statico explain` can expand on.
pub fn warn_code(code: &'static str, msg: impl Into<String>) {
    push_warning(msg.into(), Some(code));
}

fn push_warning(msg: String, code: Option<&'static str>) {
    if !quiet() {
        eprintln!("warning: {msg}");
    }
    let mut d = parse(&msg, "warning");
    d.code = code;
    let mut w = WARNINGS.lock().unwrap();
    if !w.contains(&d) {
        w.push(d);
    }
}

pub fn take_warnings() -> Vec<Diagnostic> {
    std::mem::take(&mut *WARNINGS.lock().unwrap())
}

/// Stable error codes (see `statico explain <code>`), inferred from messages.
pub fn code_for(msg: &str) -> Option<&'static str> {
    let m = msg;
    Some(if m.contains("[S201]") {
        "S201"
    } else if m.contains("is never closed")
        || m.contains("with no matching opening tag")
        || m.contains("is still open")
        || m.contains("must be inside an <If>")
        || m.contains("after <Else>")
    {
        "S001"
    } else if m.contains("include cycle") || m.contains("nest too deeply") {
        "S008"
    } else if m.contains("cannot read `")
        && (m.contains("includes/") || m.contains("layouts/") || m.contains("pages/"))
    {
        "S002"
    } else if m.contains("must include a src") || m.contains("must have a src") {
        "S003"
    } else if m.contains("invalid condition") || m.contains("invalid where=") {
        "S004"
    } else if m.contains("front matter") {
        "S005"
    } else if m.contains("no collection")
        || m.contains("cannot read collection")
        || m.contains("is not valid JSON")
        || m.contains("is not valid YAML")
        || m.contains("is not valid TOML")
        || m.contains("is not valid CSV")
        || m.contains("content folder")
    {
        "S006"
    } else if m.contains("no page template found") {
        "S007"
    } else {
        return None;
    })
}

/// Split `path/file.html:12: message` into parts.
pub fn parse(msg: &str, level: &'static str) -> Diagnostic {
    let mut d = parse_loc(msg, level);
    d.code = code_for(msg);
    d.message = d.message.replace(" [S201]", "");
    d
}

fn parse_loc(msg: &str, level: &'static str) -> Diagnostic {
    let first = msg.lines().next().unwrap_or_default();
    if let Some((loc, rest)) = first.split_once(": ") {
        let mut parts = loc.rsplitn(2, ':');
        let last = parts.next().unwrap_or_default();
        let (file, line) = match (last.parse::<usize>(), parts.next()) {
            (Ok(n), Some(f)) => (f.to_string(), Some(n)),
            _ => (loc.to_string(), None),
        };
        let looks_like_path = file.contains('.') && !file.contains(' ');
        if looks_like_path {
            let tail: String = msg.lines().skip(1).collect::<Vec<_>>().join("\n");
            let message = if tail.is_empty() {
                rest.to_string()
            } else {
                format!("{rest}\n{tail}")
            };
            return Diagnostic {
                level,
                file: Some(file),
                line,
                url: None,
                message,
                code: None,
            };
        }
    }
    Diagnostic {
        level,
        file: None,
        line: None,
        url: None,
        message: msg.to_string(),
        code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_locations() {
        let d = parse("pages/a.html:3: <If> is never closed", "error");
        assert_eq!(d.file.as_deref(), Some("pages/a.html"));
        assert_eq!(d.line, Some(3));
        assert_eq!(d.message, "<If> is never closed");
        let d = parse("content/x.md: invalid front matter YAML", "error");
        assert_eq!(d.file.as_deref(), Some("content/x.md"));
        assert_eq!(d.line, None);
        let d = parse("no location here", "error");
        assert!(d.file.is_none());
    }
}
