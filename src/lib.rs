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
pub mod token;

pub use category::{categorize, is_wrapper_type, Category};
pub use role::{roles_for_native, Role};
pub use token::TokenPolicy;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;   // BTree, not Hash: props must serialise deterministically
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<UastNode>,
}

impl UastNode {
    pub fn new(native_type: impl Into<String>, language: impl Into<String>, span: Span) -> Self {
        let native_type = native_type.into();
        let native_type_for_roles = native_type.clone();
        Self {
            category: categorize(&native_type),
            native_type,
            language: language.into(),
            span,
            props: BTreeMap::new(),
            roles: roles_for_native(&native_type_for_roles),
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
}

/// Normalise a source tree into canonical UAST.
///
/// Grammar scaffolding is collapsed: a node with no meaning of its own contributes its
/// children to the parent instead of a level of nesting. That is what makes the same code
/// produce the same UAST across grammars — Python wraps a call in `expression_statement`,
/// Go does not, and that difference must not survive normalisation.
pub fn normalize<T: SourceTree>(tree: &T, language: &str) -> UastNode {
    let mut root = UastNode::new(tree.node_type(), language, tree.span());
    root.children = normalize_children(tree, language);
    root
}

fn normalize_children<T: SourceTree>(tree: &T, language: &str) -> Vec<UastNode> {
    let mut out = Vec::new();
    for child in tree.children() {
        if is_wrapper_type(child.node_type()) {
            // Splice through: the wrapper itself is not a level of meaning.
            out.extend(normalize_children(child, language));
        } else {
            let mut node = UastNode::new(child.node_type(), language, child.span());
            node.children = normalize_children(child, language);
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
    }
    fn n(t: &'static str, c: Vec<T>) -> T {
        T { t, c }
    }
    fn leaf(t: &'static str) -> T {
        n(t, vec![])
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
            vec![n("block", vec![n("expression_statement", vec![leaf("call")])])],
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
        assert_eq!(shape(&a), vec![Category::FunctionDeclaration, Category::Call]);
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
            vec![n("block", vec![
                n("if_statement", vec![leaf("return_statement")]),
                leaf("call"),
            ])],
        );
        let wrapped = n(
            "function_definition",
            vec![n("block", vec![n("if_statement", vec![leaf("call")])])],
        );
        let g = normalize(&guard, "python");
        let w = normalize(&wrapped, "python");
        assert_ne!(g, w);
        assert!(g.find(Category::Conditional).unwrap().find(Category::Return).is_some());
        assert!(w.find(Category::Conditional).unwrap().find(Category::Return).is_none());
    }

    #[test]
    fn counting_walks_the_whole_subtree() {
        let tree = n(
            "function_definition",
            vec![n("block", vec![leaf("call"), n("if_statement", vec![leaf("call")])])],
        );
        let u = normalize(&tree, "python");
        assert_eq!(u.count(Category::Call), 2);
        assert_eq!(u.count(Category::Conditional), 1);
    }
}
