//! Placeholder filters: `{post.date|date:"%B %-d, %Y"}`, `{post.title|upper}` …

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde_json::Value;

use super::value_to_string;

pub const NAMES: &[&str] = &[
    "upper",
    "lower",
    "capitalize",
    "date",
    "truncate",
    "slug",
    "escape",
    "json",
    "default",
    "strip_html",
    "markdown",
    "length",
    "join",
    "first",
    "last",
    "reading_time",
    "abs_url",
];

/// Parse a date from front matter: `2024-01-10`, `2024-01-10 12:00`, RFC 3339.
pub fn parse_date(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.naive_utc());
    }
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Some(d);
        }
    }
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Apply one filter. Returns `None` for an unknown filter name.
pub fn apply(name: &str, arg: Option<&str>, v: Value, base_url: &str) -> Option<Value> {
    let s = || value_to_string(&v);
    Some(match name {
        "upper" => Value::String(s().to_uppercase()),
        "lower" => Value::String(s().to_lowercase()),
        "capitalize" => {
            let st = s();
            let mut c = st.chars();
            Value::String(match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            })
        }
        "date" => {
            let fmt = arg.unwrap_or("%B %-d, %Y");
            match parse_date(&s()) {
                Some(d) => Value::String(d.format(fmt).to_string()),
                None => v,
            }
        }
        "truncate" => {
            let n: usize = arg.and_then(|a| a.parse().ok()).unwrap_or(160);
            let text = s();
            if text.chars().count() <= n {
                Value::String(text)
            } else {
                let cut: String = text.chars().take(n).collect();
                let cut = match cut.rfind(' ') {
                    Some(i) if i > n / 2 => cut[..i].to_string(),
                    _ => cut,
                };
                Value::String(format!("{}…", cut.trim_end_matches([',', '.', ';', ':'])))
            }
        }
        "slug" => Value::String(slugify(&s())),
        "escape" => Value::String(escape_html(&s())),
        "json" => Value::String(v.to_string()),
        "default" => {
            if super::expr::truthy(&v) {
                v
            } else {
                Value::String(arg.unwrap_or_default().to_string())
            }
        }
        "strip_html" => Value::String(strip_html(&s())),
        "markdown" => Value::String(crate::markdown::render(&s()).html),
        "length" => Value::from(match &v {
            Value::Array(a) => a.len(),
            Value::Object(o) => o.len(),
            _ => s().chars().count(),
        }),
        "join" => match &v {
            Value::Array(a) => Value::String(
                a.iter()
                    .map(value_to_string)
                    .collect::<Vec<_>>()
                    .join(arg.unwrap_or(", ")),
            ),
            _ => v,
        },
        "first" => match &v {
            Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
            _ => v,
        },
        "last" => match &v {
            Value::Array(a) => a.last().cloned().unwrap_or(Value::Null),
            _ => v,
        },
        "reading_time" => {
            let words = strip_html(&s()).split_whitespace().count();
            Value::from(words.div_ceil(220).max(1))
        }
        "abs_url" => {
            let path = s();
            if base_url == "relative" || path.starts_with("http") {
                Value::String(path)
            } else {
                Value::String(format!("{base_url}{path}"))
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn f(name: &str, arg: Option<&str>, v: Value) -> String {
        value_to_string(&apply(name, arg, v, "https://x.dev").unwrap())
    }

    #[test]
    fn filters() {
        assert_eq!(f("date", None, json!("2024-03-02")), "March 2, 2024");
        assert_eq!(
            f("date", Some("%Y"), json!("2023-03-17T05:35:07.322Z")),
            "2023"
        );
        assert_eq!(f("upper", None, json!("hi")), "HI");
        assert_eq!(
            f("slug", None, json!("Hello, World! Ça va")),
            "hello-world-ça-va"
        );
        assert_eq!(
            f("truncate", Some("11"), json!("hello there world")),
            "hello there…"
        );
        assert_eq!(f("default", Some("n/a"), json!("")), "n/a");
        assert_eq!(f("join", Some(" / "), json!(["a", "b"])), "a / b");
        assert_eq!(f("escape", None, json!("<b>")), "&lt;b&gt;");
        assert_eq!(f("strip_html", None, json!("<p>a <b>b</b></p>")), "a b");
        assert_eq!(f("reading_time", None, json!("word ".repeat(500))), "3");
        assert_eq!(f("abs_url", None, json!("/a")), "https://x.dev/a");
        assert!(apply("nope", None, json!(1), "relative").is_none());
    }
}
