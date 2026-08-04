//! A compact, privacy-safe rendering of a UAST subtree.
//!
//! # The problem it solves
//!
//! A flat fact bag says what a function CONTAINS. It cannot say how it is ARRANGED, and
//! arrangement is usually where the intent lives:
//!
//! ```text
//! if not x: return None      guard: reject and leave
//! if x: do_work()            wrapped body
//! ```
//!
//! Both are "has a conditional and a call".
//!
//! # Why an earlier attempt failed
//!
//! A first version rendered kinds only, and collapsed the guard above to `if{return}` —
//! which told a reader nothing a boolean had not already said. The missing piece was never
//! more nesting; it was ROLES. With them the same code renders:
//!
//! ```text
//! fn(1){if[Negated]{return[EarlyExit]},call}
//! ```
//!
//! An LLM reading that can say *"rejects invalid input early"*. A deterministic matcher can
//! find the same pattern without a model at all.
//!
//! # Privacy
//!
//! Categories, roles and counts. No identifiers, no literals, no tokens — even when the
//! [`TokenPolicy`](crate::TokenPolicy) allowed the tree to carry them. That is deliberate:
//! the skeleton exists to be sendable, so it must not inherit whatever disclosure the tree
//! was built with.

use crate::{Category, UastNode};

/// Depth beyond which structure is elided. Guards, early returns and wrapped bodies all
/// live in the first few levels; deeper nesting costs prompt budget without adding intent.
const MAX_DEPTH: usize = 4;
/// Siblings kept per level. Bounds a 400-statement function to something a prompt affords.
const MAX_CHILDREN: usize = 8;

/// Short, stable name for a category. Short because this goes in prompts and is repeated
/// per change; stable because a renamed token would invalidate every cached comparison.
fn tag(category: Category) -> &'static str {
    match category {
        Category::FunctionDeclaration => "fn",
        Category::ClassDeclaration => "class",
        Category::Conditional => "if",
        Category::Loop => "loop",
        Category::Return => "return",
        Category::TryBlock => "try",
        Category::Call => "call",
        Category::Assignment => "assign",
        Category::Mapping => "map",
        Category::Sequence => "list",
        Category::KeyValuePair => "kv",
        Category::Literal => "lit",
        Category::Element => "el",
        Category::Attribute => "attr",
        Category::Section => "section",
        Category::Paragraph => "para",
        Category::ListItem => "item",
        Category::CodeBlock => "code",
        Category::Query => "query",
        Category::Clause => "clause",
        Category::File => "file",
        Category::Identifier => "id",
        Category::TypeReference => "type",
        Category::Import => "import",
        Category::Unknown => "_",
        // Deliberately EXHAUSTIVE, no catch-all. #[non_exhaustive] only binds downstream
        // crates, so within this one the compiler can enforce that every new category is
        // given a tag. A `_ => "_"` arm would let one silently render as unknown.
    }
}

/// Nodes worth rendering. Identifiers and literals are structurally uninteresting and
/// would triple the size for no signal — the shape is about control flow, not operands.
fn is_salient(category: Category) -> bool {
    !matches!(
        category,
        Category::Unknown | Category::Identifier | Category::Literal | Category::TypeReference
    )
}

fn write_node(node: &UastNode, depth: usize, out: &mut String) {
    out.push_str(tag(node.category));

    // Roles are the whole reason this rendering is useful; without them a guard and a
    // branch are the same string.
    if !node.roles.is_empty() {
        out.push('[');
        for (i, role) in node.roles.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(role.as_str());
        }
        out.push(']');
    }

    let salient: Vec<&UastNode> = node
        .children
        .iter()
        .filter(|c| is_salient(c.category))
        .collect();
    if salient.is_empty() {
        return;
    }
    if depth >= MAX_DEPTH {
        // Mark that structure continues rather than implying a leaf.
        out.push_str("{…}");
        return;
    }

    out.push('{');
    for (i, child) in salient.iter().take(MAX_CHILDREN).enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_node(child, depth + 1, out);
    }
    if salient.len() > MAX_CHILDREN {
        // An explicit count, not a bare ellipsis: "+12" tells a reader the body is large,
        // which is itself signal about the change.
        out.push_str(&format!(",+{}", salient.len() - MAX_CHILDREN));
    }
    out.push('}');
}

/// Render a subtree. `None` when nothing salient is inside, so a trivial node does not pay
/// for an empty skeleton.
pub fn skeleton(node: &UastNode) -> Option<String> {
    let mut out = String::new();
    write_node(node, 0, &mut out);
    if !out.contains('{') && node.roles.is_empty() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{normalize, SourceTree, Span};

    struct T {
        t: &'static str,
        tok: Option<&'static str>,
        c: Vec<T>,
    }
    impl SourceTree for T {
        fn node_type(&self) -> &str {
            self.t
        }
        fn span(&self) -> Span {
            Span::default()
        }
        fn children(&self) -> Vec<&Self> {
            self.c.iter().collect()
        }
        fn token(&self) -> Option<&str> {
            self.tok
        }
    }
    fn n(t: &'static str, c: Vec<T>) -> T {
        T { t, tok: None, c }
    }
    fn leaf(t: &'static str) -> T {
        n(t, vec![])
    }
    fn op(t: &'static str, tok: &'static str) -> T {
        T {
            t,
            tok: Some(tok),
            c: vec![],
        }
    }

    #[test]
    fn a_guard_clause_reads_as_a_guard_clause() {
        // THE case. The previous kind-only rendering gave `if{return}`, which said nothing
        // a boolean had not. Roles are what make this legible.
        let f = n(
            "function_definition",
            vec![n(
                "block",
                vec![
                    n(
                        "if_statement",
                        vec![
                            n("not_operator", vec![leaf("identifier")]),
                            n("block", vec![leaf("return_statement")]),
                        ],
                    ),
                    leaf("call"),
                ],
            )],
        );
        let u = normalize(&f, "python");
        let s = skeleton(&u).expect("skeleton");
        assert!(s.contains("if[Negated]"), "{s}");
        assert!(s.contains("return[EarlyExit]"), "{s}");
    }

    #[test]
    fn a_guard_and_a_wrapped_body_render_differently() {
        // The distinction the flat vocabulary could not make, now visible in one string.
        let guard = normalize(
            &n(
                "function_definition",
                vec![n(
                    "block",
                    vec![
                        n(
                            "if_statement",
                            vec![
                                op("unary_expression", "!"),
                                n("block", vec![leaf("return_statement")]),
                            ],
                        ),
                        leaf("call"),
                    ],
                )],
            ),
            "javascript",
        );
        let wrapped = normalize(
            &n(
                "function_definition",
                vec![n(
                    "block",
                    vec![n(
                        "if_statement",
                        vec![leaf("identifier"), n("block", vec![leaf("call")])],
                    )],
                )],
            ),
            "javascript",
        );
        assert_ne!(skeleton(&guard), skeleton(&wrapped));
    }

    #[test]
    fn operands_are_omitted_so_the_shape_stays_about_control_flow() {
        // Identifiers and literals would triple the size for no structural signal.
        let f = n(
            "function_definition",
            vec![n(
                "block",
                vec![n(
                    "call",
                    vec![leaf("identifier"), leaf("string"), leaf("number")],
                )],
            )],
        );
        let s = skeleton(&normalize(&f, "python")).expect("skeleton");
        assert!(!s.contains("id"), "{s}");
        assert!(!s.contains("lit"), "{s}");
    }

    #[test]
    fn breadth_is_capped_with_an_explicit_count() {
        let many: Vec<T> = (0..12).map(|_| leaf("call")).collect();
        let f = n("function_definition", vec![n("block", many)]);
        let s = skeleton(&normalize(&f, "python")).expect("skeleton");
        assert!(s.contains("+4"), "expected 12-8=4 elided: {s}");
        assert_eq!(s.matches("call").count(), MAX_CHILDREN);
    }

    #[test]
    fn depth_is_capped_without_implying_a_leaf() {
        let mut inner = n("if_statement", vec![leaf("call")]);
        for _ in 0..6 {
            inner = n("if_statement", vec![inner]);
        }
        let s = skeleton(&normalize(&inner, "python")).expect("skeleton");
        assert!(s.contains('…'), "deep nesting must elide: {s}");
    }

    #[test]
    fn the_skeleton_never_carries_source_even_when_the_tree_does() {
        // The load-bearing privacy property: a skeleton must be sendable regardless of the
        // policy the tree was built with, so it cannot inherit that disclosure.
        let f = n(
            "function_definition",
            vec![n(
                "block",
                vec![n("call", vec![op("string", "\"sk-live-SECRET\"")])],
            )],
        );
        let u = normalize_with(&f, "python", &crate::TokenPolicy::all());
        // The tree DOES carry the secret...
        assert!(
            u.descendants()
                .iter()
                .any(|d| d.token.as_deref().is_some_and(|t| t.contains("SECRET"))),
            "fixture should have a token"
        );
        // ...and the skeleton must not.
        let s = skeleton(&u).expect("skeleton");
        assert!(!s.contains("SECRET"), "skeleton leaked a token: {s}");
    }

    use crate::normalize_with;

    #[test]
    fn sql_shape_shows_which_clauses_a_query_has() {
        // Roles carry the clause kind, so "added a JOIN" is visible in the shape rather
        // than needing a separate lookup.
        let q = n(
            "select_statement",
            vec![
                leaf("select_clause"),
                leaf("join_clause"),
                leaf("where_clause"),
            ],
        );
        let s = skeleton(&normalize(&q, "tsql")).expect("skeleton");
        assert!(s.contains("clause[Joining]"), "{s}");
        assert!(s.contains("clause[Filtering]"), "{s}");
    }
}
