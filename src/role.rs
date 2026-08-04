//! Roles: semantic classifications carried ALONGSIDE a node's category.
//!
//! # Why roles instead of more categories
//!
//! A lambda IS a function that happens to be anonymous. A method IS a function that happens
//! to be a member. Model that with categories alone and you get an explosion —
//! `FunctionDeclaration`, `LambdaExpression`, `MethodDeclaration`, `ArrowFunction` — where
//! every consumer must remember to enumerate all four to mean "a function".
//!
//! Roles factor the orthogonal part out. One [`crate::Category::FunctionDeclaration`] carrying
//! `[Lambda]` or `[Declaration, Public]` answers both "is this a function?" (cheap) and
//! "is it anonymous?" (also cheap), and new distinctions are added without a new category
//! and without breaking existing queries.
//!
//! Borrowed from Babelfish's UAST and Codefang, which both converged on this after starting
//! from flat vocabularies. UAST-Grep did not, and carries `IfStatement` *and*
//! `ConditionalExpression` for one idea as a result.

use serde::{Deserialize, Serialize};

/// An orthogonal semantic classification. A node may carry several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[non_exhaustive]
pub enum Role {
    /// Introduces a name (as opposed to referencing one).
    Declaration,
    /// Has no name of its own — lambda, closure, anonymous class.
    Anonymous,
    /// Belongs to a type: method, field.
    Member,
    /// Suspends: async fn, generator, coroutine.
    Suspending,
    /// Exits the enclosing construct early — the guard-clause signal.
    EarlyExit,
    /// The condition is negated (`if not x`, `unless x`).
    ///
    /// This one exists because its absence is what made a bare shape skeleton useless:
    /// `if not x: return` and `if x: return` are the same category and the same shape, and
    /// only the negation distinguishes a guard from a branch.
    Negated,
    /// Iterates a collection rather than testing a condition (`for x in xs` vs `while`).
    Iterating,
    /// Visible outside its module.
    Public,
    /// Explicitly restricted.
    Private,
    /// Cannot be reassigned: const, final, val.
    Immutable,
    // ── Clause kinds (SQL) ──────────────────────────────────────────────────
    // Which clause a `Clause` is. Roles rather than six categories, for the same reason a
    // lambda is Function + [Anonymous]: "is this a clause?" stays one comparison, and
    // "is it a join?" is another.
    //
    // The kinds are NOT equally interesting to a diff. Adding a Joining clause changes
    // result CARDINALITY; adding a Filtering one changes which rows survive. Both are
    // meaningful. An Ordering change usually is not. Naming them separately is what lets a
    // reviewer be told which kind of change happened.
    /// SELECT — what comes back.
    Projection,
    /// FROM — where it comes from.
    Source,
    /// WHERE / HAVING — which rows survive.
    Filtering,
    /// JOIN — changes cardinality, so a diff should never treat it as cosmetic.
    Joining,
    /// GROUP BY — collapses rows.
    Grouping,
    /// ORDER BY / LIMIT — presentation, usually the least semantic change in a query.
    Ordering,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Declaration => "Declaration",
            Role::Anonymous => "Anonymous",
            Role::Member => "Member",
            Role::Suspending => "Suspending",
            Role::EarlyExit => "EarlyExit",
            Role::Negated => "Negated",
            Role::Iterating => "Iterating",
            Role::Public => "Public",
            Role::Private => "Private",
            Role::Immutable => "Immutable",
            Role::Projection => "Projection",
            Role::Source => "Source",
            Role::Filtering => "Filtering",
            Role::Joining => "Joining",
            Role::Grouping => "Grouping",
            Role::Ordering => "Ordering",
        }
    }
}

/// Roles implied by a native node type alone, before any structural analysis.
///
/// Only what the TYPE proves. Anything needing context — a return being an early exit, a
/// condition being negated — is decided during normalisation where the surrounding tree is
/// visible, not here.
pub fn roles_for_native(native: &str) -> Vec<Role> {
    let mut roles = Vec::new();
    match native {
        "lambda"
        | "lambda_expression"
        | "arrow_function"
        | "closure_expression"
        | "function_expression"
        | "anonymous_function" => {
            roles.push(Role::Anonymous);
        }
        "method_definition" | "method_declaration" | "constructor_declaration" => {
            roles.push(Role::Declaration);
            roles.push(Role::Member);
        }
        "function_definition"
        | "function_declaration"
        | "function_item"
        | "class_definition"
        | "class_declaration"
        | "struct_item"
        | "trait_item"
        | "interface_declaration" => {
            roles.push(Role::Declaration);
        }
        "generator_function" | "generator_function_declaration" => {
            roles.push(Role::Declaration);
            roles.push(Role::Suspending);
        }
        "for_statement"
        | "for_in_statement"
        | "for_expression"
        | "for_range_loop"
        | "list_comprehension"
        | "set_comprehension"
        | "dictionary_comprehension"
        | "generator_expression" => {
            roles.push(Role::Iterating);
        }
        "select_clause" | "select_item" => roles.push(Role::Projection),
        "from_clause" => roles.push(Role::Source),
        "where_clause" | "having_clause" => roles.push(Role::Filtering),
        "join_clause" => roles.push(Role::Joining),
        "group_by_clause" => roles.push(Role::Grouping),
        "order_by_clause" | "limit_clause" => roles.push(Role::Ordering),
        "unless_statement" | "guard_statement" => {
            // The grammar itself encodes the negation.
            roles.push(Role::Negated);
        }
        _ => {}
    }
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lambda_is_a_function_that_is_anonymous() {
        // The whole argument for roles: one category, the distinction in a role, so
        // "is this a function?" stays a single check.
        assert!(roles_for_native("arrow_function").contains(&Role::Anonymous));
        assert!(!roles_for_native("arrow_function").contains(&Role::Declaration));
    }

    #[test]
    fn a_method_is_both_a_declaration_and_a_member() {
        let r = roles_for_native("method_declaration");
        assert!(r.contains(&Role::Declaration));
        assert!(r.contains(&Role::Member));
    }

    #[test]
    fn iterating_loops_are_distinguished_from_conditional_ones() {
        // for/comprehension iterate; while tests. One Loop category, role carries the rest.
        assert!(roles_for_native("for_statement").contains(&Role::Iterating));
        assert!(roles_for_native("list_comprehension").contains(&Role::Iterating));
        assert!(!roles_for_native("while_statement").contains(&Role::Iterating));
    }

    #[test]
    fn grammars_that_encode_negation_carry_it() {
        assert!(roles_for_native("unless_statement").contains(&Role::Negated));
        assert!(!roles_for_native("if_statement").contains(&Role::Negated));
    }

    #[test]
    fn unknown_types_claim_nothing() {
        assert!(roles_for_native("some_exotic_node").is_empty());
    }
}
