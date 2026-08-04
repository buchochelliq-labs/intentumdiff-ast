//! The canonical vocabulary. See SPEC.md for the full 47-category schema and tiers.
//!
//! Only tier 1 is implemented; the rest are tracked as issues. Adding a category is
//! additive by design — `Unknown` is a legitimate answer, so a partial vocabulary is
//! honest rather than broken.

use serde::{Deserialize, Serialize};

/// A normalised node category. `Unknown` is not a failure: it means no category applies,
/// and the node keeps its native type so nothing is silently mis-labelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[non_exhaustive]
pub enum Category {
    // ── Tier 1 ──────────────────────────────────────────────────────────────
    FunctionDeclaration,
    ClassDeclaration,
    Conditional,
    Loop,
    Return,
    TryBlock,
    Call,
    Assignment,
    // ── Data (JSON/YAML/TOML/INI) ───────────────────────────────────────────
    Mapping,
    Sequence,
    KeyValuePair,
    Literal,
    // ── Markup + document (XML/HTML/Markdown) ───────────────────────────────
    Element,
    Attribute,
    Section,
    Paragraph,
    ListItem,
    CodeBlock,
    // ── References ──────────────────────────────────────────────────────────
    Identifier,
    TypeReference,
    Import,
    /// No category applies. Carries `native_type` so a consumer can still reason about it.
    Unknown,
}

impl Category {
    /// Every category, for policies that need to enumerate.
    pub const ALL: [Category; 22] = [
        Category::FunctionDeclaration,
        Category::ClassDeclaration,
        Category::Conditional,
        Category::Loop,
        Category::Return,
        Category::TryBlock,
        Category::Call,
        Category::Assignment,
        Category::Mapping,
        Category::Sequence,
        Category::KeyValuePair,
        Category::Literal,
        Category::Element,
        Category::Attribute,
        Category::Section,
        Category::Paragraph,
        Category::ListItem,
        Category::CodeBlock,
        Category::Identifier,
        Category::TypeReference,
        Category::Import,
        Category::Unknown,
    ];

    /// True when the category is grammar scaffolding whose children should splice into the
    /// parent. Nothing is scaffolding yet — wrapper collapse is decided by
    /// [`is_wrapper_type`], which works on the NATIVE type, since scaffolding by definition
    /// has no category.
    pub fn is_structural(self) -> bool {
        matches!(self, Category::Unknown)
    }
}

/// Grammar scaffolding: nodes that exist for parsing, not for meaning.
///
/// Collapsing these is what makes the same code produce the same UAST across grammars —
/// Python wraps a call in `expression_statement`, other grammars do not, and that
/// difference must not survive into the canonical form.
pub fn is_wrapper_type(native: &str) -> bool {
    matches!(
        native,
        "block"
            | "body"
            | "statement_block"
            | "compound_statement"
            | "expression_statement"
            | "declaration_list"
            | "source_file"
            | "module"
            | "program"
            | "document"
            | "config_file"
            | "suite"
    )
}

/// Map a native (grammar-specific) node type onto a canonical category.
///
/// Deliberately conservative: an unrecognised type is `Unknown`, never a near-miss. A wrong
/// category is worse than an honest gap because downstream reasoning cannot tell it was a
/// guess — rule 3 in SPEC.md.
pub fn categorize(native: &str) -> Category {
    match native {
        // Functions. Covers def/fn/lambda/method across the tree-sitter grammars.
        "function_definition"
        | "function_declaration"
        | "function_item"
        | "method_definition"
        | "method_declaration"
        | "constructor_declaration"
        | "lambda"
        | "lambda_expression"
        | "arrow_function"
        | "closure_expression"
        | "function_expression"
        | "generator_function_declaration"
        | "generator_function"
        | "subroutine"
        | "procedure" => Category::FunctionDeclaration,

        // Types that own members. Interfaces and traits are grouped here on purpose: for
        // structural reasoning "a named thing with members" is the useful distinction.
        "class_definition"
        | "class_declaration"
        | "class_specifier"
        | "struct_item"
        | "struct_specifier"
        | "record_declaration"
        | "trait_item"
        | "interface_declaration"
        | "object_declaration"
        | "impl_item"
        | "enum_declaration"
        | "enum_item" => Category::ClassDeclaration,

        // Conditionals, including expression-position forms.
        "if_statement"
        | "if_expression"
        | "elif_clause"
        | "else_clause"
        | "conditional_expression"
        | "ternary_expression"
        | "unless_statement"
        | "guard_statement" => Category::Conditional,

        // Iteration. Comprehensions are loops: they iterate, and treating them otherwise
        // makes "added iteration" invisible in languages that prefer them.
        "for_statement"
        | "for_expression"
        | "for_in_statement"
        | "for_range_loop"
        | "while_statement"
        | "while_expression"
        | "do_statement"
        | "loop_expression"
        | "repeat_statement"
        | "list_comprehension"
        | "set_comprehension"
        | "dictionary_comprehension"
        | "generator_expression" => Category::Loop,

        "return_statement" | "return_expression" | "return" => Category::Return,

        "try_statement" | "try_expression" | "begin_block" | "do_block" => Category::TryBlock,

        "call"
        | "call_expression"
        | "function_call"
        | "function_call_expression"
        | "method_invocation"
        | "method_call"
        | "invocation_expression"
        | "macro_invocation"
        | "new_expression"
        | "object_creation_expression" => Category::Call,

        "assignment"
        | "assignment_expression"
        | "augmented_assignment"
        | "compound_assignment_expr"
        | "let_declaration"
        | "variable_declarator"
        | "short_var_declaration" => Category::Assignment,

        // ── Data. JSON/YAML/TOML/INI are not code: no functions, no calls, just
        // mappings, sequences and scalars. Every reference we surveyed (Codefang,
        // UAST-Grep, Semgrep) has a code-centric vocabulary and no answer here, yet
        // config and data files are a large share of a real review.
        "object" | "block_mapping" | "flow_mapping" | "table" | "inline_table" | "section" => {
            Category::Mapping
        }
        "array" | "block_sequence" | "flow_sequence" => Category::Sequence,
        "pair" | "block_mapping_pair" | "flow_pair" | "setting" => Category::KeyValuePair,
        "string"
        | "number"
        | "true"
        | "false"
        | "null"
        | "integer"
        | "float"
        | "boolean"
        | "string_scalar"
        | "integer_scalar"
        | "float_scalar"
        | "boolean_scalar"
        | "null_scalar"
        | "plain_scalar"
        | "block_scalar"
        | "double_quote_scalar"
        | "single_quote_scalar"
        | "setting_value" => Category::Literal,

        // ── Markup. An XML element owns BOTH attributes and ordered children, which is
        // neither a mapping nor a sequence — it needs its own pair of categories.
        "element" | "start_tag" | "self_closing_tag" => Category::Element,
        "attribute" | "xmlns" => Category::Attribute,

        // ── Document. Markdown is prose structure: headings nest, paragraphs and list
        // items are content. Nothing in a code vocabulary describes this.
        "atx_heading" | "setext_heading" => Category::Section,
        "paragraph" => Category::Paragraph,
        "list_item" => Category::ListItem,
        "fenced_code_block" | "indented_code_block" | "code_block" => Category::CodeBlock,

        // ── References
        "identifier" | "bare_key" | "dotted_key" | "key" | "property_identifier" => {
            Category::Identifier
        }
        "type_identifier" | "type_annotation" | "primitive_type" => Category::TypeReference,
        "import_statement"
        | "import_declaration"
        | "import_from_statement"
        | "use_declaration"
        | "require_call"
        | "include_statement" => Category::Import,

        _ => Category::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_idea_normalises_across_languages() {
        // The entire point: one category for many spellings.
        for native in [
            "function_definition",  // python
            "function_declaration", // go, js
            "function_item",        // rust
            "method_declaration",   // java
            "arrow_function",       // js
        ] {
            assert_eq!(
                categorize(native),
                Category::FunctionDeclaration,
                "{native} should be a function"
            );
        }
        for native in ["if_statement", "if_expression", "unless_statement"] {
            assert_eq!(categorize(native), Category::Conditional, "{native}");
        }
        for native in ["for_statement", "while_expression", "list_comprehension"] {
            assert_eq!(categorize(native), Category::Loop, "{native}");
        }
    }

    #[test]
    fn unrecognised_types_are_unknown_not_guessed() {
        // Rule 3: an honest gap beats a wrong category, because a consumer cannot tell a
        // near-miss from a real match.
        assert_eq!(categorize("some_exotic_grammar_node"), Category::Unknown);
        assert_eq!(categorize(""), Category::Unknown);
    }

    #[test]
    fn wrappers_are_recognised_as_scaffolding() {
        for w in ["block", "expression_statement", "source_file", "suite"] {
            assert!(is_wrapper_type(w), "{w} is scaffolding");
        }
        assert!(!is_wrapper_type("if_statement"));
    }
}
