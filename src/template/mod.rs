//! Template engine: parses `<ForEach>` / `<If>` / `<ElseIf>` / `<Else>` into a
//! tree and renders it against a stack of scopes.
//!
//! Composition (`<page>`, `<layout>`, `<include>`) happens earlier, at the
//! string level, in [`compose`].

pub mod compose;
pub mod expr;
pub mod filters;
pub mod tags;

use anyhow::{Context as _, Result, bail};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

use expr::{Resolver, eval_condition};
use tags::{find_tags, line_of};

#[derive(Debug, Clone)]
pub enum Node {
    Text(String),
    ForEach {
        attrs: Vec<(String, String)>,
        body: Vec<Node>,
        line: usize,
    },
    If {
        /// `(condition, body)` pairs; a `None` condition is the `<Else>` branch.
        branches: Vec<(Option<String>, Vec<Node>)>,
        line: usize,
    },
}

enum Frame {
    Root(Vec<Node>),
    ForEach {
        attrs: Vec<(String, String)>,
        body: Vec<Node>,
        line: usize,
    },
    If {
        branches: Vec<(Option<String>, Vec<Node>)>,
        line: usize,
    },
}

impl Frame {
    fn push(&mut self, n: Node) {
        match self {
            Frame::Root(v) | Frame::ForEach { body: v, .. } => v.push(n),
            Frame::If { branches, .. } => branches.last_mut().expect("If has a branch").1.push(n),
        }
    }
}

/// Parse a composed page into a node tree. `origin` is only used for errors.
pub fn parse(src: &str, origin: &str) -> Result<Vec<Node>> {
    let tags = find_tags(src, &["ForEach", "If", "ElseIf", "Else"]);
    let mut stack = vec![Frame::Root(Vec::new())];
    let mut pos = 0;

    for t in tags {
        if t.start > pos {
            stack
                .last_mut()
                .unwrap()
                .push(Node::Text(src[pos..t.start].to_string()));
        }
        pos = t.end;
        let line = line_of(src, t.start);
        match (t.name.as_str(), t.closing) {
            ("ForEach", false) => stack.push(Frame::ForEach {
                attrs: t.attrs,
                body: Vec::new(),
                line,
            }),
            ("If", false) => {
                let cond = t
                    .attr("condition")
                    .with_context(|| {
                        format!("{origin}:{line}: <If> is missing a `condition` attribute")
                    })?
                    .to_string();
                stack.push(Frame::If {
                    branches: vec![(Some(cond), Vec::new())],
                    line,
                });
            }
            ("ElseIf", false) | ("Else", false) => {
                let Some(Frame::If { branches, .. }) = stack.last_mut() else {
                    bail!("{origin}:{line}: <{}> must be inside an <If>", t.name);
                };
                if branches.last().is_some_and(|b| b.0.is_none()) {
                    bail!("{origin}:{line}: <{}> after <Else>", t.name);
                }
                let cond = if t.name == "ElseIf" {
                    Some(
                        t.attr("condition")
                            .with_context(|| {
                                format!(
                                    "{origin}:{line}: <ElseIf> is missing a `condition` attribute"
                                )
                            })?
                            .to_string(),
                    )
                } else {
                    None
                };
                branches.push((cond, Vec::new()));
            }
            ("ElseIf", true) | ("Else", true) => {
                // `</Else>` is tolerated and ignored, so both `<Else>...</If>`
                // and `<Else>...</Else></If>` work.
            }
            (name, true) => {
                let frame = stack.pop().unwrap();
                let node = match (name, frame) {
                    ("ForEach", Frame::ForEach { attrs, body, line }) => {
                        Node::ForEach { attrs, body, line }
                    }
                    ("If", Frame::If { branches, line }) => Node::If { branches, line },
                    (_, Frame::Root(_)) => {
                        bail!("{origin}:{line}: unexpected </{name}> with no matching opening tag")
                    }
                    (_, Frame::ForEach { line: l, .. }) => {
                        bail!(
                            "{origin}:{line}: found </{name}> but the <ForEach> opened on line {l} is still open"
                        )
                    }
                    (_, Frame::If { line: l, .. }) => {
                        bail!(
                            "{origin}:{line}: found </{name}> but the <If> opened on line {l} is still open"
                        )
                    }
                };
                stack.last_mut().unwrap().push(node);
            }
            _ => unreachable!(),
        }
    }
    if pos < src.len() {
        stack
            .last_mut()
            .unwrap()
            .push(Node::Text(src[pos..].to_string()));
    }
    if stack.len() > 1 {
        let (kind, line) = match stack.last().unwrap() {
            Frame::ForEach { line, .. } => ("ForEach", *line),
            Frame::If { line, .. } => ("If", *line),
            Frame::Root(_) => unreachable!(),
        };
        bail!("{origin}:{line}: <{kind}> is never closed");
    }
    match stack.pop() {
        Some(Frame::Root(nodes)) => Ok(nodes),
        _ => unreachable!(),
    }
}

/// Where `<ForEach>` gets its data from.
pub trait DataSource: Sync {
    /// `collections/<name>.json`
    fn collection(&self, name: &str) -> Result<Value>;
    /// Front matter of every markdown file in `content/<dir>`.
    fn content_collection(&self, dir: &str) -> Result<Value>;
}

/// A stack of named scopes. Names may contain dots (`content.posts`), and
/// lookups use the innermost scope whose name is a prefix of the path.
pub struct Scope<'a> {
    vars: Vec<(String, Value)>,
    /// Fields of the current loop item, reachable as bare identifiers in conditions.
    bare: Vec<Value>,
    data: &'a dyn DataSource,
    origin: String,
    /// `relative` or the absolute base URL (for the `abs_url` filter).
    pub base_url: String,
    /// Which page of a `<ForEach paginate="N">` to render (1-based).
    pub page_number: usize,
    /// Set by the first paginated `<ForEach>`: (items per page, total items).
    pub paginated: Option<(usize, usize)>,
}

impl<'a> Scope<'a> {
    pub fn new(data: &'a dyn DataSource, origin: impl Into<String>) -> Self {
        Scope {
            vars: Vec::new(),
            bare: Vec::new(),
            data,
            origin: origin.into(),
            base_url: "relative".into(),
            page_number: 1,
            paginated: None,
        }
    }

    pub fn set(&mut self, name: impl Into<String>, v: Value) {
        self.vars.push((name.into(), v));
    }

    pub fn lookup(&self, path: &str) -> Option<Value> {
        for (name, v) in self.vars.iter().rev() {
            if path == name {
                return Some(v.clone());
            }
            if let Some(rest) = path
                .strip_prefix(name.as_str())
                .and_then(|r| r.strip_prefix('.'))
            {
                return lookup_path(v, rest);
            }
        }
        // `{collections.menu.json}` inlines a collection as JSON.
        if let Some(name) = path
            .strip_prefix("collections.")
            .and_then(|r| r.strip_suffix(".json"))
        {
            return self
                .data
                .collection(name)
                .ok()
                .map(|v| Value::String(v.to_string()));
        }
        None
    }
}

impl Resolver for Scope<'_> {
    fn resolve(&self, path: &str) -> Option<Value> {
        self.lookup(path)
            .or_else(|| self.bare.last().and_then(|item| lookup_path(item, path)))
    }
}

/// Walk `a.b.0.c` into a JSON value. `length` works on arrays, strings and objects.
pub fn lookup_path(v: &Value, path: &str) -> Option<Value> {
    let segs: Vec<&str> = path.split('.').collect();
    let mut cur = v;
    for (i, seg) in segs.iter().enumerate() {
        let next = match cur {
            Value::Object(m) => m.get(*seg),
            Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        };
        match next {
            Some(n) => cur = n,
            None if *seg == "length" && i == segs.len() - 1 => {
                return match cur {
                    Value::Array(a) => Some(Value::from(a.len())),
                    Value::String(s) => Some(Value::from(s.chars().count())),
                    Value::Object(m) => Some(Value::from(m.len())),
                    _ => None,
                };
            }
            None => return None,
        }
    }
    Some(cur.clone())
}

/// How a value appears when substituted into HTML (matches the JS original:
/// arrays are joined with `|`, numbers print without a trailing `.0`).
pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e15 => (f as i64).to_string(),
            _ => n.to_string(),
        },
        Value::Array(a) => a.iter().map(value_to_string).collect::<Vec<_>>().join("|"),
        Value::Object(_) => v.to_string(),
    }
}

static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"\{([A-Za-z_$][\w$-]*(?:\.[\w$-]+)*)((?:\s*\|\s*[a-z_]+(?::(?:"[^"]*"|'[^']*'|[^|}\s]+))?)*)\s*\}"#,
    )
    .unwrap()
});
static FILTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\|\s*([a-z_]+)(?::(?:"([^"]*)"|'([^']*)'|([^|}\s]+)))?"#).unwrap()
});

/// Replace `{a.b}` placeholders whose root is a known variable. Unknown
/// placeholders (e.g. inline JS like `{open}`) are left untouched.
pub fn substitute(text: &str, scope: &Scope) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    PLACEHOLDER
        .replace_all(text, |c: &regex::Captures| {
            let mut v = match scope.lookup(&c[1]) {
                Some(v) => v,
                // `{post.subtitle|default:"—"}`: a missing field of a known
                // variable is null when filters follow, so `default` works.
                None if !c[2].trim().is_empty()
                    && c[1]
                        .split_once('.')
                        .is_some_and(|(root, _)| scope.lookup(root).is_some()) =>
                {
                    Value::Null
                }
                None => return c[0].to_string(),
            };
            for f in FILTER.captures_iter(&c[2]) {
                let arg = f.get(2).or(f.get(3)).or(f.get(4)).map(|m| m.as_str());
                match filters::apply(&f[1], arg, v, &scope.base_url) {
                    Some(nv) => v = nv,
                    // Unknown filter: leave the placeholder as written.
                    None => return c[0].to_string(),
                }
            }
            value_to_string(&v)
        })
        .into_owned()
}

pub fn render(nodes: &[Node], scope: &mut Scope, out: &mut String) -> Result<()> {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(&substitute(t, scope)),
            Node::If { branches, line } => {
                for (cond, body) in branches {
                    let hit = match cond {
                        None => true,
                        Some(c) => eval_condition(c, scope).with_context(|| {
                            format!("{}:{line}: invalid condition `{c}`", scope.origin)
                        })?,
                    };
                    if hit {
                        render(body, scope, out)?;
                        break;
                    }
                }
            }
            Node::ForEach { attrs, body, line } => render_foreach(attrs, body, *line, scope, out)?,
        }
    }
    Ok(())
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn render_foreach(
    attrs: &[(String, String)],
    body: &[Node],
    line: usize,
    scope: &mut Scope,
    out: &mut String,
) -> Result<()> {
    let origin = scope.origin.clone();
    let ctx = || format!("{origin}:{line}: <ForEach>");

    // Resolve the data and the default loop variable name.
    let (items, default_alias) = if let Some(dir) = attr(attrs, "content") {
        (
            scope.data.content_collection(dir).with_context(ctx)?,
            format!("content.{}", dir.replace('/', ".")),
        )
    } else if let Some(name) = attr(attrs, "collection") {
        // A variable already in scope (e.g. `post.tags`) enables nested loops.
        let from_scope = substitute_braces(name, scope);
        let root_in_scope = from_scope
            .split_once('.')
            .is_some_and(|(root, _)| scope.lookup(root).is_some());
        if root_in_scope {
            // e.g. `post.tags`: missing or null means "nothing to loop over".
            let v = scope.lookup(&from_scope).unwrap_or(Value::Null);
            (v, name.replace('/', "."))
        } else if let Some(v @ Value::Array(_)) = scope.lookup(&from_scope) {
            (v, name.replace('/', "."))
        } else if let Some(dir) = name.strip_prefix("content/") {
            (
                scope.data.content_collection(dir).with_context(ctx)?,
                name.replace('/', "."),
            )
        } else {
            (
                scope.data.collection(name).with_context(ctx)?,
                name.replace('/', "."),
            )
        }
    } else {
        bail!("{}: needs a `collection` or `content` attribute", ctx());
    };

    let mut items = match items {
        Value::Array(a) => a,
        Value::Object(m) => m
            .into_iter()
            .map(|(k, v)| {
                // Iterating an object yields `{key, value}` pairs.
                serde_json::json!({ "key": k, "value": v })
            })
            .collect(),
        Value::Null => Vec::new(),
        other => bail!("{}: data is not a list (got {other})", ctx()),
    };

    let alias = attr(attrs, "as")
        .map(str::to_string)
        .unwrap_or(default_alias);

    if let Some(order) = attr(attrs, "orderBy") {
        order_by(&mut items, order);
    }

    // `where` filters items with the same expression language as <If>.
    if let Some(cond) = attr(attrs, "where") {
        let mut kept = Vec::with_capacity(items.len());
        for item in items {
            scope.vars.push((alias.clone(), item.clone()));
            scope.bare.push(item.clone());
            let ok = eval_condition(cond, scope);
            scope.vars.pop();
            scope.bare.pop();
            if ok.with_context(|| format!("{}: invalid where=\"{cond}\"", ctx()))? {
                kept.push(item);
            }
        }
        items = kept;
    }

    let offset = attr(attrs, "offset")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let count = attr(attrs, "count").and_then(|s| s.trim().parse::<usize>().ok());
    let total = items.len();
    let mut selected: Vec<Value> = items
        .into_iter()
        .skip(offset)
        .take(count.unwrap_or(usize::MAX))
        .collect();
    // `paginate="10"`: the page renders one slice; the build emits /page/2/ etc.
    if let Some(per) = attr(attrs, "paginate").and_then(|s| s.trim().parse::<usize>().ok()) {
        let per = per.max(1);
        if scope.paginated.is_none() {
            scope.paginated = Some((per, selected.len()));
        }
        let start = (scope.page_number.saturating_sub(1)) * per;
        selected = selected.into_iter().skip(start).take(per).collect();
    }
    let n = selected.len();

    for (i, item) in selected.into_iter().enumerate() {
        let index = i + 1;
        scope.vars.push((alias.clone(), item.clone()));
        scope.vars.push(("loop".into(), Value::from(index)));
        scope.vars.push((
            "$loop".into(),
            serde_json::json!({"index": index, "index0": i, "first": i == 0, "last": index == n, "count": n, "total": total}),
        ));
        scope.bare.push(item);
        let r = render(body, scope, out);
        scope.bare.pop();
        scope.vars.truncate(scope.vars.len() - 3);
        r?;
    }
    Ok(())
}

/// Allow `collection="{post.tags}"` as well as `collection="post.tags"`.
fn substitute_braces(s: &str, _scope: &Scope) -> String {
    s.trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim()
        .to_string()
}

/// `orderBy="date"` or `orderBy="date, desc"`. Strings compare
/// lexicographically, numbers numerically; the sort is stable.
pub fn order_by(items: &mut [Value], spec: &str) {
    let mut parts = spec.split(',').map(str::trim);
    let key = parts.next().unwrap_or_default().to_string();
    let desc = parts.next().is_some_and(|d| d.eq_ignore_ascii_case("desc"));
    items.sort_by(|a, b| {
        let (va, vb) = (lookup_path(a, &key), lookup_path(b, &key));
        let ord = match (&va, &vb) {
            (Some(Value::Number(x)), Some(Value::Number(y))) => x
                .as_f64()
                .partial_cmp(&y.as_f64())
                .unwrap_or(std::cmp::Ordering::Equal),
            (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
            _ => std::cmp::Ordering::Equal,
        };
        if desc { ord.reverse() } else { ord }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Data;
    impl DataSource for Data {
        fn collection(&self, name: &str) -> Result<Value> {
            match name {
                "menu" => Ok(
                    json!([{"title": "Home", "link": "/"}, {"title": "About", "link": "/about", "tags": ["x", "y"]}]),
                ),
                "nums" => Ok(json!([{"n": 3}, {"n": 1}, {"n": 2}])),
                _ => bail!("no collection {name}"),
            }
        }
        fn content_collection(&self, _dir: &str) -> Result<Value> {
            Ok(json!([{"title": "B", "date": "2024-02-01"}, {"title": "A", "date": "2024-01-01"}]))
        }
    }

    fn run(src: &str) -> String {
        let nodes = parse(src, "test").unwrap();
        let mut scope = Scope::new(&Data, "test");
        scope.set("frontmatter", json!({"title": "T", "draft": false}));
        let mut out = String::new();
        render(&nodes, &mut scope, &mut out).unwrap();
        out
    }

    #[test]
    fn foreach_basic() {
        assert_eq!(
            run(r#"<ForEach collection="menu"><li>{menu.title}</li></ForEach>"#),
            "<li>Home</li><li>About</li>"
        );
    }

    #[test]
    fn foreach_as_count_order_conditions() {
        let s = r#"<ForEach collection="nums" as="x" orderBy="n, desc" count="2"><If condition="loop == 1">[{x.n}]</If><If condition="loop > 1">{x.n}</If></ForEach>"#;
        assert_eq!(run(s), "[3]2");
    }

    #[test]
    fn content_alias_and_bare_keys() {
        assert_eq!(
            run(
                r#"<ForEach content="posts" orderBy="date"><If condition="title == 'A'">{content.posts.title}</If></ForEach>"#
            ),
            "A"
        );
    }

    #[test]
    fn nested_loops() {
        let s = r#"<ForEach collection="menu" as="m"><ForEach collection="m.tags" as="t">{m.title}:{t};</ForEach></ForEach>"#;
        assert_eq!(run(s), "About:x;About:y;");
    }

    #[test]
    fn else_branches() {
        assert_eq!(
            run(
                r#"<If condition="frontmatter.draft">D<ElseIf condition="frontmatter.title == 'T'">T<Else>E</If>"#
            ),
            "T"
        );
        assert_eq!(
            run(r#"<If condition="frontmatter.draft">D<Else/>E</If>"#),
            "E"
        );
    }

    #[test]
    fn unknown_placeholders_untouched() {
        assert_eq!(
            run(r#"<div x-data="{open}">{frontmatter.title}{nope.x}</div>"#),
            r#"<div x-data="{open}">T{nope.x}</div>"#
        );
    }

    #[test]
    fn where_offset_loop_meta() {
        let s = r#"<ForEach collection="nums" as="x" where="x.n >= 2" offset="1">{x.n}{$loop.last}</ForEach>"#;
        assert_eq!(run(s), "2true");
    }

    #[test]
    fn filters_and_pagination() {
        assert_eq!(
            run(
                r#"{frontmatter.title|lower} {frontmatter.title | default:"x"} {frontmatter.title|bogus}"#
            ),
            "t T {frontmatter.title|bogus}"
        );
        let nodes = parse(
            r#"<ForEach collection="nums" as="x" paginate="2">{x.n}</ForEach>"#,
            "t",
        )
        .unwrap();
        let mut scope = Scope::new(&Data, "t");
        scope.page_number = 2;
        let mut out = String::new();
        render(&nodes, &mut scope, &mut out).unwrap();
        assert_eq!(out, "2");
        assert_eq!(scope.paginated, Some((2, 3)));
    }

    #[test]
    fn unbalanced_errors_have_lines() {
        let e = parse("a\n<ForEach collection=\"menu\">\nx", "p.html")
            .unwrap_err()
            .to_string();
        assert!(e.contains("p.html:2"), "{e}");
    }

    #[test]
    fn unbalanced_html_inside_if_is_fine() {
        // The official blog template opens a <section> in one <If> and closes it in another.
        let s = r#"<ForEach collection="nums" as="x"><If condition="loop==1"><section></If><If condition="loop==3"></section></If></ForEach>"#;
        assert_eq!(run(s), "<section></section>");
    }
}
