//! Canonical cross-language AST (UAST).
//!
//! One normalised vocabulary over many grammars, so a Python `def`, a Java
//! `public static void` and a Rust `fn` are all [`Category::FunctionDeclaration`].
//!
//! # Where this sits
//!
//! ```text
//! source bytes
//!    │  tree-sitter grammar          (per language — 69 of them)
//!    ▼
//!   CST     native_type = "if_statement"
//!    │  intentumdiff_ast::normalize  ← this crate
//!    ▼
//!  UAST     category = Conditional, native_type retained
//! ```
//!
//! tree-sitter is the FRONTEND; this crate is the normalising layer above it. The direction
//! matters: the CST is per-grammar and every consumer that wants to reason across languages
//! otherwise re-derives the same mapping. IntentumDiff derived it three times before this
//! crate existed.
//!
//! # Position is preserved
//!
//! Every node keeps its byte range. That is not incidental — it is what lets a consumer map
//! a canonical finding back onto real source, and what would make a tree-sitter-style query
//! layer over UAST possible later.

pub mod category;
pub mod role;
pub mod structural;
pub mod token;

pub use category::{categorize, is_wrapper_type, Category};
pub use role::{roles_for_native, Role};
pub use token::TokenPolicy;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap; // BTree, not Hash: props must serialise deterministically
                                // or two identical trees would not compare equal.

/// A half-open byte range into the original source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_row: u32,
    pub end_row: u32,
}

/// A node in the canonical tree.
///
/// `native_type` is retained deliberately. Babelfish's most-copied decision was keeping the
/// native AST alongside the universal one, so a consumer needing grammar detail is never
/// forced back to re-parsing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UastNode {
    pub category: Category,
    /// What the grammar called it. Provenance, and the escape hatch when a category is
    /// `Unknown`.
    pub native_type: String,
    /// Language id of the producing grammar, e.g. "python".
    pub language: String,
    pub span: Span,
    /// Category-specific structural properties: `negated`, `loop_kind`, `operator`, …
    ///
    /// This is where the design earns its keep over a bare kind. `if not x` and `if x` are
    /// the SAME category and differ only here — without props a guard clause is
    /// indistinguishable from a wrapped body, which is the gap that motivated this crate.
    /// Prior art (UAST-Grep, Babelfish) carries kind only and has the same blind spot.
    ///
    /// Structural values ONLY — enums and flags, never an identifier or a literal. That is
    /// what keeps a UAST safe to send where raw source is not.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, String>,
    /// Orthogonal semantic classifications — see [`Role`].
    ///
    /// A node may hold several. This is what keeps the category vocabulary small: a lambda
    /// is `Function` + `[Anonymous]`, not a separate `LambdaExpression` category, so
    /// "is this a function?" stays one comparison instead of an enumeration.
    ///
    /// NOTE the deliberate omission: unlike Codefang and Babelfish there is no `token`
    /// field carrying source text. Categories, roles and structural props only — that is
    /// exactly what lets a UAST be sent somewhere raw source cannot go.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role>,
    /// Literal source text, carried ONLY where [`TokenPolicy`] permits it for this node's
    /// category. `None` under the default policy.
    ///
    /// This is the one field that can carry secrets, PII or proprietary logic verbatim, so
    /// it is opt-in per category and absent unless explicitly allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<UastNode>,
}

impl UastNode {
    pub fn new(native_type: impl Into<String>, language: impl Into<String>, span: Span) -> Self {
        let native_type = native_type.into();
        let native_type_for_roles = native_type.clone();
        let language = language.into();
        Self {
            // Language-aware: the same native type can mean different things in different
            // grammars (INI `section` is a mapping, Markdown `section` is a document
            // section), so the category cannot be decided from the type alone.
            category: categorize(&native_type, &language),
            native_type,
            language,
            span,
            props: BTreeMap::new(),
            roles: roles_for_native(&native_type_for_roles),
            token: None,
            children: Vec::new(),
        }
    }

    /// Depth-first iterator over descendants, excluding self.
    pub fn descendants(&self) -> Vec<&UastNode> {
        let mut out = Vec::new();
        let mut stack: Vec<&UastNode> = self.children.iter().rev().collect();
        while let Some(node) = stack.pop() {
            out.push(node);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    /// First descendant (or self) matching a category.
    pub fn find(&self, category: Category) -> Option<&UastNode> {
        if self.category == category {
            return Some(self);
        }
        self.descendants()
            .into_iter()
            .find(|n| n.category == category)
    }

    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    pub fn count(&self, category: Category) -> usize {
        let own = usize::from(self.category == category);
        own + self
            .descendants()
            .iter()
            .filter(|n| n.category == category)
            .count()
    }
}

/// A minimal source tree, as produced by any tree-sitter grammar.
///
/// Deliberately NOT a tree_sitter::Node: this crate does not depend on tree-sitter. The
/// normaliser works on any structure that can describe itself as (type, span, children),
/// so a caller can feed it a real CST, a serialised one, or a fixture. That keeps the
/// canonical vocabulary independent of any one parser generator.
pub trait SourceTree {
    fn node_type(&self) -> &str;
    fn span(&self) -> Span;
    fn children(&self) -> Vec<&Self>
    where
        Self: Sized;

    /// The literal source text of this node, if the caller can supply it.
    ///
    /// On the trait rather than passed alongside as raw bytes: the tree already knows its
    /// own spans, and making callers re-supply source invites an off-by-one between a span
    /// and the buffer it indexes. Defaults to `None` so fixtures need not implement it.
    fn token(&self) -> Option<&str> {
        None
    }
}

/// Normalise a source tree into canonical UAST.
///
/// Grammar scaffolding is collapsed: a node with no meaning of its own contributes its
/// children to the parent instead of a level of nesting. That is what makes the same code
/// produce the same UAST across grammars — Python wraps a call in `expression_statement`,
/// Go does not, and that difference must not survive normalisation.
pub fn normalize<T: SourceTree>(tree: &T, language: &str) -> UastNode {
    normalize_with(tree, language, &TokenPolicy::none())
}

/// Normalise, carrying source tokens where `policy` permits.
///
/// Structure is identical whatever the policy; only [`UastNode::token`] varies. That
/// property is what lets one normalisation serve both a local consumer and a cloud one.
pub fn normalize_with<T: SourceTree>(tree: &T, language: &str, policy: &TokenPolicy) -> UastNode {
    let mut root = UastNode::new(tree.node_type(), language, tree.span());
    apply_token(&mut root, tree, policy);
    apply_source_roles(&mut root, tree);
    root.children = normalize_children(tree, language, policy);
    // Tail position is a property of the FINISHED tree: it needs wrappers already collapsed
    // and every child in place, so it runs last rather than during the walk.
    structural::mark_early_exits(&mut root);
    root
}

fn apply_token<T: SourceTree>(node: &mut UastNode, source: &T, policy: &TokenPolicy) {
    if policy.permits(node.category) {
        node.token = source.token().map(str::to_owned);
    }
}

/// Roles that need the SOURCE tree, decided before its detail is normalised away.
///
/// Negation lives in the operator, and wrapper collapse plus categorisation discard exactly
/// that, so it has to be read here rather than recovered from the finished UAST.
fn apply_source_roles<T: SourceTree>(node: &mut UastNode, source: &T) {
    if node.category == Category::Conditional
        && structural::condition_is_negated(source)
        && !node.roles.contains(&Role::Negated)
    {
        node.roles.push(Role::Negated);
    }
}

fn normalize_children<T: SourceTree>(
    tree: &T,
    language: &str,
    policy: &TokenPolicy,
) -> Vec<UastNode> {
    let mut out = Vec::new();
    for child in tree.children() {
        if is_wrapper_type(child.node_type()) {
            // Splice through: the wrapper itself is not a level of meaning.
            out.extend(normalize_children(child, language, policy));
        } else {
            let mut node = UastNode::new(child.node_type(), language, child.span());
            apply_token(&mut node, child, policy);
            apply_source_roles(&mut node, child);
            node.children = normalize_children(child, language, policy);
            out.push(node);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test tree standing in for any tree-sitter CST.
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
    /// A leaf carrying source text, for policy tests.
    fn tok(t: &'static str, text: &'static str) -> T {
        T {
            t,
            tok: Some(text),
            c: vec![],
        }
    }

    /// A tree with a secret in a literal and a name in an identifier — the two things a
    /// policy must be able to separate.
    fn sensitive() -> T {
        n(
            "function_definition",
            vec![n(
                "block",
                vec![
                    tok("identifier", "charge_customer"),
                    tok("string", "sk-live-SECRET-VALUE"),
                ],
            )],
        )
    }

    fn tokens(u: &UastNode) -> Vec<String> {
        std::iter::once(u)
            .chain(u.descendants())
            .filter_map(|n| n.token.clone())
            .collect()
    }

    #[test]
    fn categories_are_assigned_from_native_types() {
        let tree = n("function_definition", vec![leaf("return_statement")]);
        let u = normalize(&tree, "python");
        assert_eq!(u.category, Category::FunctionDeclaration);
        assert_eq!(u.children[0].category, Category::Return);
        // Provenance survives.
        assert_eq!(u.native_type, "function_definition");
        assert_eq!(u.language, "python");
    }

    #[test]
    fn wrappers_collapse_so_grammars_agree() {
        // Python nests a call inside expression_statement inside block; Go would not. Both
        // must normalise to the same shape or cross-language reasoning is impossible.
        let pythonish = n(
            "function_definition",
            vec![n(
                "block",
                vec![n("expression_statement", vec![leaf("call")])],
            )],
        );
        let goish = n("function_declaration", vec![leaf("call_expression")]);

        let a = normalize(&pythonish, "python");
        let b = normalize(&goish, "go");

        let shape = |u: &UastNode| -> Vec<Category> {
            std::iter::once(u.category)
                .chain(u.descendants().iter().map(|d| d.category))
                .collect()
        };
        assert_eq!(shape(&a), shape(&b));
        assert_eq!(
            shape(&a),
            vec![Category::FunctionDeclaration, Category::Call]
        );
    }

    #[test]
    fn unknown_nodes_keep_their_native_type() {
        // An honest gap: categorised as Unknown, but nothing is lost — a consumer can still
        // read the grammar's own name for it.
        let tree = n("some_exotic_node", vec![]);
        let u = normalize(&tree, "cobol");
        assert_eq!(u.category, Category::Unknown);
        assert_eq!(u.native_type, "some_exotic_node");
    }

    #[test]
    fn guard_clause_is_structurally_distinct() {
        // The case that motivated the crate: a negated early return vs a wrapped body are
        // different SHAPES, and the canonical tree must keep them different.
        let guard = n(
            "function_definition",
            vec![n(
                "block",
                vec![
                    n("if_statement", vec![leaf("return_statement")]),
                    leaf("call"),
                ],
            )],
        );
        let wrapped = n(
            "function_definition",
            vec![n("block", vec![n("if_statement", vec![leaf("call")])])],
        );
        let g = normalize(&guard, "python");
        let w = normalize(&wrapped, "python");
        assert_ne!(g, w);
        assert!(g
            .find(Category::Conditional)
            .unwrap()
            .find(Category::Return)
            .is_some());
        assert!(w
            .find(Category::Conditional)
            .unwrap()
            .find(Category::Return)
            .is_none());
    }

    #[test]
    fn the_default_policy_carries_no_tokens_at_all() {
        // The load-bearing privacy guarantee. A leak here is silent and unrecoverable once
        // the data has left, so this must fail loudly rather than subtly.
        let u = normalize(&sensitive(), "python");
        assert!(
            tokens(&u).is_empty(),
            "default policy leaked tokens: {:?}",
            tokens(&u)
        );
    }

    #[test]
    fn signatures_carry_names_but_never_literal_values() {
        let u = normalize_with(&sensitive(), "python", &TokenPolicy::signatures());
        let got = tokens(&u);
        assert!(
            got.contains(&"charge_customer".to_owned()),
            "want the identifier: {got:?}"
        );
        assert!(
            !got.iter().any(|t| t.contains("SECRET")),
            "signatures must never carry a literal value: {got:?}"
        );
    }

    #[test]
    fn deny_beats_a_blanket_allow_end_to_end() {
        // "everything except literals" is the policy most reviews actually want.
        let u = normalize_with(
            &sensitive(),
            "python",
            &TokenPolicy::all().deny(Category::Literal),
        );
        let got = tokens(&u);
        assert!(got.contains(&"charge_customer".to_owned()));
        assert!(
            !got.iter().any(|t| t.contains("SECRET")),
            "denied literal leaked: {got:?}"
        );
    }

    #[test]
    fn policy_changes_tokens_only_never_structure() {
        // One normalisation must serve a local consumer and a cloud one, so the shape has
        // to be identical and only the tokens may differ.
        let strip = |mut u: UastNode| -> UastNode {
            u.token = None;
            u.children = u.children.into_iter().map(strip_rec).collect();
            u
        };
        fn strip_rec(mut u: UastNode) -> UastNode {
            u.token = None;
            u.children = u.children.into_iter().map(strip_rec).collect();
            u
        }
        let bare = normalize(&sensitive(), "python");
        let full = normalize_with(&sensitive(), "python", &TokenPolicy::all());
        assert_ne!(bare, full, "policies must actually differ");
        assert_eq!(bare, strip(full), "structure must be identical");
    }

    #[test]
    fn a_guard_clause_is_recognised_in_every_grammar() {
        // THE acceptance test for #6. Before it, Negated came only from grammars that spell
        // it (unless/guard), so this worked in Ruby and Swift and failed everywhere else.
        //
        //     if not x: return None
        //     work(x)
        //
        // written five ways, all of which must yield Conditional[Negated] > Return[EarlyExit].
        let cases: Vec<(&str, T)> = vec![
            (
                "python",
                n(
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
                ),
            ),
            (
                "javascript",
                n(
                    "function_declaration",
                    vec![n(
                        "statement_block",
                        vec![
                            n(
                                "if_statement",
                                vec![
                                    T {
                                        t: "unary_expression",
                                        tok: Some("!"),
                                        c: vec![],
                                    },
                                    n("statement_block", vec![leaf("return_statement")]),
                                ],
                            ),
                            leaf("call_expression"),
                        ],
                    )],
                ),
            ),
            (
                "go",
                n(
                    "function_declaration",
                    vec![n(
                        "block",
                        vec![
                            n(
                                "if_statement",
                                vec![
                                    T {
                                        t: "unary_expression",
                                        tok: Some("!"),
                                        c: vec![],
                                    },
                                    n("block", vec![leaf("return_statement")]),
                                ],
                            ),
                            leaf("call_expression"),
                        ],
                    )],
                ),
            ),
            (
                "rust",
                n(
                    "function_item",
                    vec![n(
                        "block",
                        vec![
                            n(
                                "if_expression",
                                vec![
                                    T {
                                        t: "unary_expression",
                                        tok: Some("!"),
                                        c: vec![],
                                    },
                                    n("block", vec![leaf("return_expression")]),
                                ],
                            ),
                            leaf("call_expression"),
                        ],
                    )],
                ),
            ),
            (
                "java",
                n(
                    "method_declaration",
                    vec![n(
                        "block",
                        vec![
                            n(
                                "if_statement",
                                vec![
                                    T {
                                        t: "unary_expression",
                                        tok: Some("!"),
                                        c: vec![],
                                    },
                                    n("block", vec![leaf("return_statement")]),
                                ],
                            ),
                            leaf("method_invocation"),
                        ],
                    )],
                ),
            ),
        ];

        for (lang, tree) in cases {
            let u = normalize(&tree, lang);
            let cond = u
                .find(Category::Conditional)
                .unwrap_or_else(|| panic!("{lang}: no conditional"));
            assert!(
                cond.has_role(Role::Negated),
                "{lang}: guard condition should be Negated, roles={:?}",
                cond.roles
            );
            let ret = cond
                .find(Category::Return)
                .unwrap_or_else(|| panic!("{lang}: no return inside the guard"));
            assert!(
                ret.has_role(Role::EarlyExit),
                "{lang}: guarded return should be an EarlyExit, roles={:?}",
                ret.roles
            );
        }
    }

    #[test]
    fn roles_do_not_depend_on_the_token_policy() {
        // Roles are structural FACTS. If they varied with disclosure, a cloud-safe UAST
        // would silently be a worse UAST — reading source to derive structure and carrying
        // source into output are different things.
        let tree = n(
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
        let bare = normalize(&tree, "python");
        let full = normalize_with(&tree, "python", &TokenPolicy::all());
        let roles = |u: &UastNode| -> Vec<Vec<Role>> {
            std::iter::once(u)
                .chain(u.descendants())
                .map(|n| n.roles.clone())
                .collect()
        };
        assert_eq!(roles(&bare), roles(&full), "policy must not change roles");
        assert!(bare
            .find(Category::Conditional)
            .unwrap()
            .has_role(Role::Negated));
    }

    #[test]
    fn equivalent_json_and_yaml_normalise_identically() {
        // THE acceptance test for the data family (#5). The same document:
        //
        //     {"name": "svc", "ports": [80, 443]}      name: svc
        //                                              ports:
        //                                                - 80
        //                                                - 443
        //
        // If these do not agree, the canonical vocabulary is not canonical, and a diff
        // between a JSON file and its YAML equivalent would show spurious changes.
        let json = n(
            "document",
            vec![n(
                "object",
                vec![
                    n("pair", vec![leaf("key"), leaf("string")]),
                    n(
                        "pair",
                        vec![
                            leaf("key"),
                            n("array", vec![leaf("number"), leaf("number")]),
                        ],
                    ),
                ],
            )],
        );
        let yaml = n(
            "document",
            vec![n(
                "block_mapping",
                vec![
                    n(
                        "block_mapping_pair",
                        vec![leaf("key"), leaf("plain_scalar")],
                    ),
                    n(
                        "block_mapping_pair",
                        vec![
                            leaf("key"),
                            n(
                                "block_sequence",
                                vec![leaf("plain_scalar"), leaf("plain_scalar")],
                            ),
                        ],
                    ),
                ],
            )],
        );

        let shape = |u: &UastNode| -> Vec<Category> {
            std::iter::once(u)
                .chain(u.descendants())
                .map(|n| n.category)
                .collect()
        };
        let a = normalize(&json, "json");
        let b = normalize(&yaml, "yaml");
        assert_eq!(
            shape(&a),
            shape(&b),
            "equivalent documents must produce one canonical shape"
        );
        // And that shape is meaningful, not uniformly Unknown.
        assert_eq!(a.count(Category::KeyValuePair), 2);
        assert_eq!(a.count(Category::Sequence), 1);
        assert!(
            !shape(&a).contains(&Category::Unknown),
            "no Unknown in a covered format"
        );
    }

    #[test]
    fn markdown_and_ini_sections_do_not_collide() {
        // Same native type, different grammars, different meanings — decided by language.
        let md = normalize(&n("section", vec![leaf("paragraph")]), "markdown");
        let ini = normalize(&n("section", vec![leaf("setting")]), "ini");
        assert_eq!(md.category, Category::Section);
        assert_eq!(ini.category, Category::Mapping);
    }

    #[test]
    fn an_xml_element_owns_both_attributes_and_ordered_children() {
        // The question #5 posed: is `element` its own category, or just a KeyValuePair?
        // It is its own, because it carries BOTH keyed attributes and ordered content —
        // which is neither a Mapping nor a Sequence.
        let xml = n(
            "element",
            vec![
                leaf("attribute"),
                leaf("attribute"),
                n("element", vec![leaf("content")]),
            ],
        );
        let u = normalize(&xml, "xml");
        assert_eq!(u.category, Category::Element);
        assert_eq!(u.count(Category::Attribute), 2);
        assert!(u.find(Category::Element).is_some());
    }

    #[test]
    fn counting_walks_the_whole_subtree() {
        let tree = n(
            "function_definition",
            vec![n(
                "block",
                vec![leaf("call"), n("if_statement", vec![leaf("call")])],
            )],
        );
        let u = normalize(&tree, "python");
        assert_eq!(u.count(Category::Call), 2);
        assert_eq!(u.count(Category::Conditional), 1);
    }
}
