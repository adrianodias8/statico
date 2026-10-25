//! A tiny, safe expression evaluator for `<If condition="...">`.
//!
//! The original implementation ran conditions through `new Function(...)`,
//! i.e. arbitrary JavaScript evaluation. This evaluator supports the subset
//! that templates actually use, with JavaScript-like semantics:
//!
//! * literals: numbers, `'strings'`, `"strings"`, `true`, `false`, `null`
//! * variables: `loop`, `post.title`, `frontmatter.draft`, `{post.title}`,
//!   `post.tags.length`, `items.0.name`
//! * operators: `==` `===` `!=` `!==` `<` `<=` `>` `>=` `&&` `||` `!` `( )`
//! * helpers: `x contains y` (substring / array membership)

use anyhow::{Result, bail};
use serde_json::Value;

pub trait Resolver {
    fn resolve(&self, path: &str) -> Option<Value>;
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn tokenize(src: &str) -> Result<Vec<Tok>> {
    let src = decode_entities(src);
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Three-char operators first, then two, then one.
        let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
        let mut matched = false;
        for op in [
            "===", "!==", "==", "!=", "<=", ">=", "&&", "||", "<", ">", "!",
        ] {
            if rest.starts_with(op) {
                out.push(Tok::Op(op));
                i += op.chars().count();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '\'' | '"' => {
                let q = c;
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != q {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    bail!("unterminated string in condition `{src}`");
                }
                i += 1;
                out.push(Tok::Str(s));
            }
            c if c.is_ascii_digit()
                || (c == '-'
                    && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())
                    && !matches!(
                        out.last(),
                        Some(Tok::Num(_) | Tok::Str(_) | Tok::Ident(_) | Tok::RParen)
                    )) =>
            {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                out.push(Tok::Num(
                    s.parse().map_err(|_| anyhow::anyhow!("bad number `{s}`"))?,
                ));
            }
            '{' => {
                // `{post.title}` placeholder syntax is accepted inside conditions.
                let start = i + 1;
                let end = chars[start..]
                    .iter()
                    .position(|c| *c == '}')
                    .map(|p| start + p)
                    .ok_or_else(|| anyhow::anyhow!("unclosed `{{` in condition `{src}`"))?;
                out.push(Tok::Ident(
                    chars[start..end]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string(),
                ));
                i = end + 1;
            }
            c if c.is_alphabetic() || c == '_' || c == '$' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '.'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                out.push(match word.as_str() {
                    "and" => Tok::Op("&&"),
                    "or" => Tok::Op("||"),
                    "not" => Tok::Op("!"),
                    "contains" => Tok::Op("contains"),
                    _ => Tok::Ident(word),
                });
            }
            _ => bail!("unexpected character `{c}` in condition `{src}`"),
        }
    }
    Ok(out)
}

struct Parser<'a, R: Resolver + ?Sized> {
    toks: Vec<Tok>,
    pos: usize,
    r: &'a R,
}

impl<R: Resolver + ?Sized> Parser<'_, R> {
    fn peek_op(&self) -> Option<&'static str> {
        match self.toks.get(self.pos) {
            Some(Tok::Op(o)) => Some(o),
            _ => None,
        }
    }

    fn or(&mut self) -> Result<Value> {
        let mut l = self.and()?;
        while self.peek_op() == Some("||") {
            self.pos += 1;
            let r = self.and()?;
            l = if truthy(&l) { l } else { r };
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Value> {
        let mut l = self.cmp()?;
        while self.peek_op() == Some("&&") {
            self.pos += 1;
            let r = self.cmp()?;
            l = if truthy(&l) { r } else { l };
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<Value> {
        let l = self.unary()?;
        if let Some(op) = self.peek_op()
            && matches!(
                op,
                "==" | "===" | "!=" | "!==" | "<" | "<=" | ">" | ">=" | "contains"
            )
        {
            self.pos += 1;
            let r = self.unary()?;
            let b = match op {
                "==" => loose_eq(&l, &r),
                "===" => l == r,
                "!=" => !loose_eq(&l, &r),
                "!==" => l != r,
                "contains" => contains(&l, &r),
                _ => {
                    let ord = compare(&l, &r);
                    match (op, ord) {
                        (_, None) => false,
                        ("<", Some(o)) => o.is_lt(),
                        ("<=", Some(o)) => o.is_le(),
                        (">", Some(o)) => o.is_gt(),
                        (">=", Some(o)) => o.is_ge(),
                        _ => false,
                    }
                }
            };
            return Ok(Value::Bool(b));
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Value> {
        if self.peek_op() == Some("!") {
            self.pos += 1;
            let v = self.unary()?;
            return Ok(Value::Bool(!truthy(&v)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Value> {
        let tok = self.toks.get(self.pos).cloned();
        self.pos += 1;
        match tok {
            Some(Tok::Num(n)) => Ok(serde_json::Number::from_f64(n)
                .map(Value::Number)
                .unwrap_or(Value::Null)),
            Some(Tok::Str(s)) => Ok(Value::String(s)),
            Some(Tok::Ident(id)) => Ok(match id.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "null" | "undefined" => Value::Null,
                _ => self.r.resolve(&id).unwrap_or(Value::Null),
            }),
            Some(Tok::LParen) => {
                let v = self.or()?;
                if self.toks.get(self.pos) != Some(&Tok::RParen) {
                    bail!("expected `)`");
                }
                self.pos += 1;
                Ok(v)
            }
            Some(t) => bail!("unexpected token {t:?}"),
            None => bail!("unexpected end of condition"),
        }
    }
}

/// Evaluate `src` and return its truthiness.
pub fn eval_condition<R: Resolver + ?Sized>(src: &str, r: &R) -> Result<bool> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        bail!("empty condition");
    }
    let mut p = Parser { toks, pos: 0, r };
    let v = p.or()?;
    if p.pos != p.toks.len() {
        bail!("unexpected trailing input in condition `{src}`");
    }
    Ok(truthy(&v))
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Number(_), _)
        | (_, Value::Number(_))
        | (Value::Bool(_), _)
        | (_, Value::Bool(_)) => match (as_num(a), as_num(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        },
        _ => a == b,
    }
}

fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    if let (Value::String(x), Value::String(y)) = (a, b) {
        return Some(x.cmp(y));
    }
    as_num(a)?.partial_cmp(&as_num(b)?)
}

fn contains(haystack: &Value, needle: &Value) -> bool {
    match haystack {
        Value::String(s) => match needle {
            Value::String(n) => s.contains(n.as_str()),
            other => s.contains(&super::value_to_string(other)),
        },
        Value::Array(items) => items.iter().any(|i| loose_eq(i, needle)),
        Value::Object(map) => needle.as_str().is_some_and(|k| map.contains_key(k)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct R(Value);
    impl Resolver for R {
        fn resolve(&self, path: &str) -> Option<Value> {
            super::super::lookup_path(&self.0, path)
        }
    }

    fn ev(src: &str) -> bool {
        let r = R(
            json!({"loop": 2, "post": {"title": "Hi", "tags": ["a", "b"], "draft": false}, "n": "3"}),
        );
        eval_condition(src, &r).unwrap()
    }

    #[test]
    fn basics() {
        assert!(ev("loop == 2"));
        assert!(ev("loop==2"));
        assert!(!ev("loop === '2'"));
        assert!(ev("loop > 1 && loop < 3"));
        assert!(ev("loop &gt; 1"));
        assert!(ev("!post.draft"));
        assert!(ev("{post.title} == 'Hi'"));
        assert!(ev("post.tags.length == 2"));
        assert!(ev("post.tags contains 'b'"));
        assert!(ev("n == 3"));
        assert!(ev("(loop == 1 || loop == 2) and not post.draft"));
        assert!(!ev("missing"));
        assert!(ev("missing == null"));
        assert!(ev("loop != -1"));
    }
}
