//! Context-derived roles: the ones a node type alone cannot prove.
//!
//! [`roles_for_native`](crate::roles_for_native()) handles what the TYPE proves —
//! `arrow_function` is `Anonymous`, full stop. This module handles what needs the
//! surrounding tree: whether a condition is negated, and whether a return exits early.
//!
//! # Why this seam exists
//!
//! Before this, `Negated` was set only where the GRAMMAR spelled it (`unless_statement`,
//! `guard_statement`). So guard-clause detection worked in Ruby and Swift and failed in
//! Python, Go, Java, JavaScript and Rust — which is to say it failed. `if not x: return`
//! and `if x: return` were indistinguishable, and that distinction is the whole difference
//! between an explainer saying *"added a guard clause"* and *"added a conditional"*.
//!
//! # Reading source is not the same as carrying it
//!
//! Negation detection inspects operator text (`!`, `not`) via
//! [`SourceTree::token`]. That is deliberate and is NOT governed
//! by [`TokenPolicy`](crate::TokenPolicy):
//!
//! - **Deriving** structure from source is always allowed — the result is a boolean role.
//! - **Carrying** source into the output is what the policy governs.
//!
//! Conflating the two would make role accuracy depend on a privacy setting, so a
//! cloud-safe UAST would silently become a worse UAST. Roles are structural facts; they
//! must not vary with disclosure.

use crate::{Category, Role, SourceTree, UastNode};

/// Operator spellings that negate, across the grammars.
///
/// Token-based because node types are ambiguous here: C-family grammars emit
/// `unary_expression` for both `!x` and `-x`, so the type alone cannot decide.
const NEGATION_TOKENS: [&str; 6] = ["!", "not", "NOT", "Not", "!=", "isnt"];

/// Node types that are inherently a negation, whatever their operator text.
const NEGATION_TYPES: [&str; 4] = ["not_operator", "negated_expression", "not", "unless"];

fn is_negation<T: SourceTree>(node: &T) -> bool {
    if NEGATION_TYPES.contains(&node.node_type()) {
        return true;
    }
    // `!=` and `is not` negate a comparison; a bare `-` does not. Only trust the token when
    // the node is operator-shaped, so an identifier literally named "not" cannot trip it.
    let operator_shaped = node.node_type().contains("unary")
        || node.node_type().contains("operator")
        || node.node_type().contains("comparison");
    operator_shaped
        && node
            .token()
            .is_some_and(|t| NEGATION_TOKENS.contains(&t.trim()))
}

/// True when a conditional's test is negated.
///
/// Scans only the condition side. Descending the whole subtree would find any `!` in the
/// body and call the guard negated, which is worse than not detecting it — a wrong role is
/// indistinguishable from a right one downstream.
pub(crate) fn condition_is_negated<T: SourceTree>(conditional: &T) -> bool {
    // The test is whatever precedes the body; grammars vary, so scan children shallowly and
    // stop at the first block-like child rather than assuming a field name.
    for child in conditional.children() {
        let t = child.node_type();
        if crate::is_wrapper_type(t) {
            break; // reached the body — everything after this is not the condition
        }
        if is_negation(child) {
            return true;
        }
        // One level down: `not (a and b)` wraps the negation in a parenthesised expression.
        for grandchild in child.children() {
            if is_negation(grandchild) {
                return true;
            }
        }
    }
    false
}

/// Mark every `Return` that is not in tail position with [`Role::EarlyExit`].
///
/// Tail position is the last-child chain from the function down. A return anywhere else has
/// code after it that it skips — which is exactly what makes it *early*.
///
/// ```text
/// def f(x):          func children = [if, return]
///     if not x:        index 0 of 2 -> NOT tail -> its return is an EarlyExit
///         return None
///     return work(x)   index 1 of 2 -> tail     -> not an early exit
/// ```
///
/// A conditional return with nothing after it is deliberately NOT an early exit: it skips
/// nothing, so calling it "early" would overstate what the code does.
pub(crate) fn mark_early_exits(node: &mut UastNode) {
    mark(node, true);
}

fn mark(node: &mut UastNode, in_tail: bool) {
    if node.category == Category::Return && !in_tail && !node.roles.contains(&Role::EarlyExit) {
        node.roles.push(Role::EarlyExit);
    }
    let last = node.children.len().saturating_sub(1);
    for (i, child) in node.children.iter_mut().enumerate() {
        // Tail-ness is inherited only through the last child: a return in the final branch
        // of a final conditional is still a tail return.
        mark(child, in_tail && i == last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{normalize, Span};

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
    fn python_style_not_operator_is_negation() {
        let cond = n(
            "if_statement",
            vec![n("not_operator", vec![leaf("identifier")])],
        );
        assert!(condition_is_negated(&cond));
    }

    #[test]
    fn c_family_bang_is_negation_via_the_operator_token() {
        // `unary_expression` alone cannot decide - it covers `-x` too - so the token is
        // what distinguishes them.
        let negated = n("if_statement", vec![op("unary_expression", "!")]);
        let negative = n("if_statement", vec![op("unary_expression", "-")]);
        assert!(condition_is_negated(&negated));
        assert!(
            !condition_is_negated(&negative),
            "negation is not arithmetic negation"
        );
    }

    #[test]
    fn an_identifier_named_not_does_not_trip_detection() {
        // Only operator-shaped nodes are trusted, or a variable called `not` would make
        // every condition look negated.
        let cond = n("if_statement", vec![op("identifier", "not")]);
        assert!(!condition_is_negated(&cond));
    }

    #[test]
    fn negation_in_the_body_is_not_a_negated_condition() {
        // The bug this guards: descending the whole subtree would find the `!` inside the
        // body and wrongly call the conditional negated.
        let cond = n(
            "if_statement",
            vec![
                leaf("identifier"),
                n("block", vec![n("call", vec![op("unary_expression", "!")])]),
            ],
        );
        assert!(!condition_is_negated(&cond));
    }

    #[test]
    fn a_return_with_code_after_it_is_an_early_exit() {
        //   if not x: return None
        //   return work(x)
        let f = n(
            "function_definition",
            vec![n(
                "block",
                vec![
                    n("if_statement", vec![leaf("return_statement")]),
                    leaf("return_statement"),
                ],
            )],
        );
        let mut u = normalize(&f, "python");
        mark_early_exits(&mut u);

        let returns: Vec<&UastNode> = u
            .descendants()
            .into_iter()
            .filter(|n| n.category == Category::Return)
            .collect();
        assert_eq!(returns.len(), 2);
        assert!(
            returns[0].has_role(Role::EarlyExit),
            "the guarded return exits early"
        );
        assert!(
            !returns[1].has_role(Role::EarlyExit),
            "the final return does not"
        );
    }

    #[test]
    fn a_conditional_return_with_nothing_after_it_is_not_early() {
        // It skips nothing, so calling it "early" would overstate the code.
        let f = n(
            "function_definition",
            vec![n(
                "block",
                vec![n("if_statement", vec![leaf("return_statement")])],
            )],
        );
        let mut u = normalize(&f, "python");
        mark_early_exits(&mut u);
        let r = u.find(Category::Return).expect("a return");
        assert!(!r.has_role(Role::EarlyExit));
    }

    #[test]
    fn early_exit_and_negation_are_independent() {
        // `if x: return` then more code -> EarlyExit but NOT Negated. Conflating them would
        // make every early return look like a guard.
        let cond = n("if_statement", vec![leaf("identifier")]);
        assert!(!condition_is_negated(&cond));
        let f = n(
            "function_definition",
            vec![n(
                "block",
                vec![
                    n("if_statement", vec![leaf("return_statement")]),
                    leaf("call"),
                ],
            )],
        );
        let mut u = normalize(&f, "python");
        mark_early_exits(&mut u);
        assert!(u.find(Category::Return).unwrap().has_role(Role::EarlyExit));
    }
}
