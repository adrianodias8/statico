//! Property tests for the parts of the template engine that read what users
//! write: the `<If condition>` expression language, the tag scanner, and the
//! `<ForEach>`/`<If>` tree parser.

use proptest::prelude::*;
use serde_json::{Value, json};
use statico::template::expr::{Resolver, eval_condition};
use statico::template::tags::{find_tags, scan_tag};
use statico::template::{DataSource, Node, Scope, parse, render};

// ---------------------------------------------------------------- conditions

/// An expression tree, rendered to source and evaluated two ways: by Statico,
/// and by the tiny tree-walker below. Disagreement means the parser got the
/// structure wrong (precedence, associativity, parentheses, keywords).
#[derive(Debug, Clone)]
enum E {
    Bool(bool),
    Num(u8),
    Str(&'static str),
    Not(Box<E>),
    And(Box<E>, Box<E>),
    Or(Box<E>, Box<E>),
    Cmp(&'static str, Box<E>, Box<E>),
    Contains(&'static str, &'static str),
}

const STRS: &[&str] = &["", "a", "b", "ab", "1", "2"];
const CMPS: &[&str] = &["==", "===", "!=", "!==", "<", "<=", ">", ">="];

fn expr() -> impl Strategy<Value = E> {
    let leaf = prop_oneof![
        any::<bool>().prop_map(E::Bool),
        (0u8..4).prop_map(E::Num),
        prop::sample::select(STRS).prop_map(E::Str),
        (prop::sample::select(STRS), prop::sample::select(STRS))
            .prop_map(|(h, n)| E::Contains(h, n)),
    ];
    leaf.prop_recursive(6, 48, 2, |inner| {
        prop_oneof![
            inner.clone().prop_map(|e| E::Not(Box::new(e))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| E::And(Box::new(a), Box::new(b))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| E::Or(Box::new(a), Box::new(b))),
            (prop::sample::select(CMPS), inner.clone(), inner).prop_map(|(op, a, b)| E::Cmp(
                op,
                Box::new(a),
                Box::new(b)
            )),
        ]
    })
}

/// Binding strength, loosest first, matching the grammar: `||`, `&&`,
/// comparisons, unary `!`, atoms.
fn prec(e: &E) -> u8 {
    match e {
        E::Or(..) => 1,
        E::And(..) => 2,
        E::Cmp(..) | E::Contains(..) => 3,
        E::Not(_) => 4,
        _ => 5,
    }
}

/// Source with only the parentheses the grammar needs, so precedence is
/// actually exercised. `words` picks `and`/`or`/`not` over `&&`/`||`/`!`.
fn show(e: &E, words: bool) -> String {
    let sub = |c: &E, min: u8| {
        let s = show(c, words);
        if prec(c) < min { format!("( {s} )") } else { s }
    };
    let (and, or, not) = if words {
        (" and ", " or ", "not ")
    } else {
        (" && ", " || ", "!")
    };
    match e {
        E::Bool(b) => b.to_string(),
        E::Num(n) => n.to_string(),
        E::Str(s) => format!("'{s}'"),
        E::Contains(h, n) => format!("'{h}' contains '{n}'"),
        E::Not(a) => format!("{not}{}", sub(a, 4)),
        E::And(a, b) => format!("{}{and}{}", sub(a, 2), sub(b, 3)),
        E::Or(a, b) => format!("{}{or}{}", sub(a, 1), sub(b, 2)),
        // Both sides of a comparison are unary expressions; chains need parens.
        E::Cmp(op, a, b) => format!("{} {op} {}", sub(a, 4), sub(b, 4)),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum V {
    B(bool),
    N(f64),
    S(String),
}

fn truthy(v: &V) -> bool {
    match v {
        V::B(b) => *b,
        V::N(n) => *n != 0.0,
        V::S(s) => !s.is_empty(),
    }
}

/// Numeric coercion as in JavaScript, except that an empty string is not a
/// number (JS treats `'' == 0` as true; Statico deliberately doesn't).
fn num(v: &V) -> Option<f64> {
    match v {
        V::B(b) => Some(if *b { 1.0 } else { 0.0 }),
        V::N(n) => Some(*n),
        V::S(s) => s.trim().parse().ok(),
    }
}

fn loose_eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::S(x), V::S(y)) => x == y,
        _ => matches!((num(a), num(b)), (Some(x), Some(y)) if x == y),
    }
}

fn oracle(e: &E) -> V {
    match e {
        E::Bool(b) => V::B(*b),
        E::Num(n) => V::N(f64::from(*n)),
        E::Str(s) => V::S(s.to_string()),
        E::Contains(h, n) => V::B(h.contains(n)),
        E::Not(a) => V::B(!truthy(&oracle(a))),
        // `&&` and `||` return an operand, not a boolean, as in JavaScript.
        E::And(a, b) => {
            let l = oracle(a);
            if truthy(&l) { oracle(b) } else { l }
        }
        E::Or(a, b) => {
            let l = oracle(a);
            if truthy(&l) { l } else { oracle(b) }
        }
        E::Cmp(op, a, b) => {
            let (l, r) = (oracle(a), oracle(b));
            let ord = match (&l, &r) {
                (V::S(x), V::S(y)) => Some(x.cmp(y)),
                _ => num(&l).zip(num(&r)).and_then(|(x, y)| x.partial_cmp(&y)),
            };
            V::B(match *op {
                "==" => loose_eq(&l, &r),
                "!=" => !loose_eq(&l, &r),
                "===" => l == r,
                "!==" => l != r,
                "<" => ord.is_some_and(|o| o.is_lt()),
                "<=" => ord.is_some_and(|o| o.is_le()),
                ">" => ord.is_some_and(|o| o.is_gt()),
                ">=" => ord.is_some_and(|o| o.is_ge()),
                _ => unreachable!(),
            })
        }
    }
}

struct NoVars;
impl Resolver for NoVars {
    fn resolve(&self, _: &str) -> Option<Value> {
        None
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn conditions_match_a_reference_evaluator(e in expr(), words in any::<bool>()) {
        let src = show(&e, words);
        let got = eval_condition(&src, &NoVars);
        prop_assert!(got.is_ok(), "`{src}` failed to parse: {:?}", got);
        prop_assert_eq!(got.unwrap(), truthy(&oracle(&e)), "`{}`", src);
    }

    #[test]
    fn conditions_never_panic(src in "[ -~é😀]{0,60}") {
        let _ = eval_condition(&src, &NoVars);
    }

    #[test]
    fn conditions_from_operator_soup_never_panic(
        parts in prop::collection::vec(
            prop::sample::select(&[
                "(", ")", "!", "&&", "||", "==", "===", "<", ">=", "contains", "and",
                "not", "'s'", "\"t\"", "'", "1", "-2", "0.5", "x.y", "{a.b}", "{", "}",
                "&gt;", "&amp;&amp;", " ",
            ][..]),
            0..24,
        )
    ) {
        let _ = eval_condition(&parts.concat(), &NoVars);
    }
}

#[test]
fn deeply_nested_conditions_do_not_overflow_the_stack() {
    // A template author can write this, so it must fail cleanly rather than
    // abort the build. Run on a thread with rayon's default 2 MiB stack,
    // which is what renders pages.
    for src in [
        format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000)),
        format!("{}1", "!".repeat(100_000)),
    ] {
        let r = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || eval_condition(&src, &NoVars).is_err())
            .unwrap()
            .join();
        assert_eq!(r.ok(), Some(true), "should be a clean error");
    }
}

// ---------------------------------------------------------------- tag scanner

const NAMES: &[&str] = &["If", "ElseIf", "Else", "ForEach", "include", "layout"];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Any attribute value survives a round trip, including the characters the
    /// original regex parser choked on (`>`, `<`, `/`, `=`), and non-ASCII.
    #[test]
    fn tags_round_trip(
        name in prop::sample::select(NAMES),
        attrs in prop::collection::vec(("[a-z][a-z0-9-]{0,8}", "[^\"]{0,20}", any::<bool>()), 0..5),
        self_closing in any::<bool>(),
        before in "[^<]{0,10}",
    ) {
        let mut src = format!("{before}<{name}");
        for (k, v, single) in &attrs {
            if *single && !v.contains('\'') {
                src.push_str(&format!(" {k}='{v}'"));
            } else {
                src.push_str(&format!(" {k}=\"{v}\""));
            }
        }
        src.push_str(if self_closing { " />" } else { ">" });
        let tag = scan_tag(&src, before.len(), NAMES);
        prop_assert!(tag.is_some(), "no tag in {src:?}");
        let tag = tag.unwrap();
        prop_assert_eq!(&tag.name, name);
        prop_assert_eq!(tag.end, src.len());
        prop_assert_eq!(tag.self_closing, self_closing);
        let want: Vec<(String, String)> =
            attrs.iter().map(|(k, v, _)| (k.clone(), v.clone())).collect();
        prop_assert_eq!(tag.attrs, want);
    }

    /// Tags found in arbitrary text are in order, don't overlap, and start at a
    /// `<` on a character boundary.
    #[test]
    fn find_tags_is_well_formed(src in "([^<]{0,6}|<|</|<If|<ForEach|<Else|>|\"|'|=|/|é|😀| ){0,40}") {
        let tags = find_tags(&src, NAMES);
        let mut prev_end = 0;
        for t in &tags {
            prop_assert!(t.start >= prev_end && t.start < t.end && t.end <= src.len());
            prop_assert!(src.is_char_boundary(t.start) && src.is_char_boundary(t.end));
            prop_assert!(src[t.start..].starts_with('<'));
            prev_end = t.end;
        }
    }
}

// ---------------------------------------------------------------- tree parser

/// A well-nested template, generated as a tree so the expected shape is known.
#[derive(Debug, Clone)]
enum T {
    Text(String),
    ForEach(Vec<T>),
    If(Vec<Vec<T>>, Option<Vec<T>>),
}

fn template() -> impl Strategy<Value = Vec<T>> {
    let text = "[a-z é{}.]{1,8}".prop_map(T::Text);
    let node = text.prop_recursive(5, 40, 4, |inner| {
        let body = prop::collection::vec(inner, 0..3);
        prop_oneof![
            body.clone().prop_map(T::ForEach),
            (
                prop::collection::vec(body.clone(), 1..3),
                prop::option::of(body)
            )
                .prop_map(|(b, e)| T::If(b, e)),
        ]
    });
    prop::collection::vec(node, 0..4)
}

fn write(nodes: &[T], out: &mut String) {
    for n in nodes {
        match n {
            T::Text(s) => out.push_str(s),
            T::ForEach(body) => {
                out.push_str(r#"<ForEach collection="items" as="it">"#);
                write(body, out);
                out.push_str("</ForEach>");
            }
            T::If(branches, otherwise) => {
                for (i, b) in branches.iter().enumerate() {
                    out.push_str(if i == 0 {
                        r#"<If condition="it.n > 1 && x != 'a>b'">"#
                    } else {
                        r#"<ElseIf condition="it.n == 1">"#
                    });
                    write(b, out);
                }
                if let Some(e) = otherwise {
                    out.push_str("<Else>");
                    write(e, out);
                }
                out.push_str("</If>");
            }
        }
    }
}

/// Shape of a parsed tree, with adjacent text merged as the generator can
/// produce two text nodes in a row.
fn shape_of_parsed(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            Node::Text(_) => {
                if !s.ends_with('t') {
                    s.push('t');
                }
            }
            Node::ForEach { body, .. } => s.push_str(&format!("F[{}]", shape_of_parsed(body))),
            Node::If { branches, .. } => {
                s.push_str("I[");
                for (c, b) in branches {
                    s.push_str(if c.is_some() { "c:" } else { "e:" });
                    s.push_str(&shape_of_parsed(b));
                    s.push('|');
                }
                s.push(']');
            }
        }
    }
    s
}

fn shape_of_generated(nodes: &[T]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            T::Text(_) => {
                if !s.ends_with('t') {
                    s.push('t');
                }
            }
            T::ForEach(body) => s.push_str(&format!("F[{}]", shape_of_generated(body))),
            T::If(branches, otherwise) => {
                s.push_str("I[");
                for b in branches {
                    s.push_str("c:");
                    s.push_str(&shape_of_generated(b));
                    s.push('|');
                }
                if let Some(e) = otherwise {
                    s.push_str("e:");
                    s.push_str(&shape_of_generated(e));
                    s.push('|');
                }
                s.push(']');
            }
        }
    }
    s
}

struct Items;
impl DataSource for Items {
    fn collection(&self, name: &str) -> anyhow::Result<Value> {
        match name {
            "items" => Ok(json!([{"n": 1}, {"n": 2}])),
            _ => anyhow::bail!("no collection {name}"),
        }
    }
    fn content_collection(&self, _: &str) -> anyhow::Result<Value> {
        Ok(json!([]))
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Every well-nested template parses, into exactly the tree it was
    /// generated from, and renders without error.
    #[test]
    fn nested_templates_parse_to_their_shape(t in template()) {
        let mut src = String::new();
        write(&t, &mut src);
        let nodes = parse(&src, "prop");
        prop_assert!(nodes.is_ok(), "{src:?}: {:?}", nodes.err());
        let nodes = nodes.unwrap();
        prop_assert_eq!(shape_of_parsed(&nodes), shape_of_generated(&t), "{}", src);
        let mut scope = Scope::new(&Items, "prop");
        scope.set("x", json!("a"));
        let mut out = String::new();
        let rendered = render(&nodes, &mut scope, &mut out);
        prop_assert!(rendered.is_ok(), "{src:?}: {:?}", rendered.err());
    }

    /// Tag soup either parses or fails with an error; it never panics, and
    /// whatever parses also renders without panicking.
    #[test]
    fn tag_soup_never_panics(
        parts in prop::collection::vec(
            prop::sample::select(&[
                "<If condition=\"x\">", "<If>", "</If>", "<Else>", "</Else>",
                "<ElseIf condition=\"1\">", "<ForEach collection=\"items\">",
                "<ForEach>", "</ForEach>", "<ForEach collection=\"nope\">",
                "text", "é", "{x}", "<", ">", "\"", "<If condition=\"(\">",
            ][..]),
            0..16,
        )
    ) {
        let src = parts.concat();
        if let Ok(nodes) = parse(&src, "soup") {
            let mut scope = Scope::new(&Items, "soup");
            let mut out = String::new();
            let _ = render(&nodes, &mut scope, &mut out);
        }
    }
}
