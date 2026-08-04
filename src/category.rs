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
    /// The root of a parsed file.
    ///
    /// Root wrapper types (`document`, `source_file`, `program`, …) collapse when they
    /// appear as CHILDREN, but the root itself is kept: it is the only node representing
    /// the file as a whole, and leaving it `Unknown` made every tree contain one
    /// meaningless node.
    File,
    // ── References ──────────────────────────────────────────────────────────
    Identifier,
    TypeReference,
    Import,
    /// No category applies. Carries `native_type` so a consumer can still reason about it.
    Unknown,
}

impl Category {
    /// Every category, for policies that need to enumerate.
    pub const ALL: [Category; 23] = [
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
        Category::File,
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

/// Map a native node type onto a canonical category, in the context of its language.
///
/// The `language` parameter is load-bearing, not decoration. Node types are **grammar
/// scoped**, so the same name can mean different things: INI emits `section` for a
/// `[section]` block (a [`Category::Mapping`]) while Markdown emits `section` for a
/// document section (a [`Category::Section`]). A global table silently mis-categorises one
/// of them, which is precisely the "wrong category is worse than an honest gap" failure.
///
/// Most types are unambiguous, so the common path is a single global table with a small
/// per-language override in front of it.
pub fn categorize(native: &str, language: &str) -> Category {
    if let Some(specific) = categorize_for_language(native, language) {
        return specific;
    }
    categorize_global(native)
}

/// Overrides for native types whose meaning depends on the grammar that produced them.
///
/// Keep this small and evidenced. Every entry should name the real collision it resolves —
/// a speculative override is a mis-categorisation waiting to happen.
fn categorize_for_language(native: &str, language: &str) -> Option<Category> {
    match (language, native) {
        // `section`: a Markdown document section vs an INI/TOML `[section]` mapping.
        ("markdown" | "mdx", "section") => Some(Category::Section),
        ("ini" | "toml" | "cfg", "section") => Some(Category::Mapping),
        _ => None,
    }
}

fn categorize_global(native: &str) -> Category {
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
        "object" | "block_mapping" | "flow_mapping" | "inline_table" | "section" => {
            Category::Mapping
        }
        "array"
        | "block_sequence"
        | "flow_sequence"
        | "block_sequence_item"
        | "flow_sequence_item"
        | "table_array_element" => Category::Sequence,
        "pair" | "block_mapping_pair" | "flow_pair" | "setting" | "table" => Category::KeyValuePair,
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
        "element" | "start_tag" | "self_closing_tag" | "content" => Category::Element,
        "attribute" | "xmlns" => Category::Attribute,

        // ── Document. Markdown is prose structure: headings nest, paragraphs and list
        // items are content. Nothing in a code vocabulary describes this.
        "atx_heading" | "setext_heading" => Category::Section,
        "paragraph" | "inline" => Category::Paragraph,
        "block_quote" => Category::Paragraph,
        "list_item" => Category::ListItem,
        "fenced_code_block" | "indented_code_block" | "code_block" => Category::CodeBlock,

        // Root wrapper types. These also appear in is_wrapper_type so they COLLAPSE when
        // nested; this arm only takes effect for the root, which is never collapsed.
        "document" | "source_file" | "program" | "config_file" | "module" => Category::File,

        // ── References
        "identifier"
        | "bare_key"
        | "dotted_key"
        | "key"
        | "property_identifier"
        | "section_name"
        | "setting_name" => Category::Identifier,
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
                categorize(native, "python"),
                Category::FunctionDeclaration,
                "{native} should be a function"
            );
        }
        for native in ["if_statement", "if_expression", "unless_statement"] {
            assert_eq!(
                categorize(native, "python"),
                Category::Conditional,
                "{native}"
            );
        }
        for native in ["for_statement", "while_expression", "list_comprehension"] {
            assert_eq!(categorize(native, "python"), Category::Loop, "{native}");
        }
    }

    #[test]
    fn unrecognised_types_are_unknown_not_guessed() {
        // Rule 3: an honest gap beats a wrong category, because a consumer cannot tell a
        // near-miss from a real match.
        assert_eq!(
            categorize("some_exotic_grammar_node", "cobol"),
            Category::Unknown
        );
        assert_eq!(categorize("", "cobol"), Category::Unknown);
    }

    #[test]
    fn wrappers_are_recognised_as_scaffolding() {
        for w in ["block", "expression_statement", "source_file", "suite"] {
            assert!(is_wrapper_type(w), "{w} is scaffolding");
        }
        assert!(!is_wrapper_type("if_statement"));
    }
}

#[cfg(test)]
mod non_code_coverage_tests {
    use super::*;

    /// Node types taken from the ACTUAL parser sources, not invented. If a salient one here
    /// maps to `Unknown`, the corresponding diff carries no structural meaning at all.
    #[test]
    fn real_parser_node_types_are_covered() {
        let cases: &[(&str, &[&str])] = &[
            (
                "json",
                &[
                    "object", "array", "pair", "string", "number", "true", "false", "null",
                ],
            ),
            (
                "yaml",
                &[
                    "block_mapping_pair",
                    "block_sequence",
                    "block_sequence_item",
                    "flow_pair",
                    "flow_sequence",
                    "plain_scalar",
                    "double_quote_scalar",
                    "single_quote_scalar",
                    "key",
                ],
            ),
            (
                "toml",
                &[
                    "pair",
                    "table_array_element",
                    "inline_table",
                    "bare_key",
                    "string",
                ],
            ),
            ("xml", &["element", "attribute", "content"]),
            (
                "markdown",
                &[
                    "atx_heading",
                    "setext_heading",
                    "section",
                    "paragraph",
                    "inline",
                    "list_item",
                    "fenced_code_block",
                    "block_quote",
                ],
            ),
            (
                "ini",
                &["section", "setting", "setting_name", "setting_value"],
            ),
        ];

        let mut uncovered = Vec::new();
        for (language, types) in cases {
            for native in *types {
                if categorize(native, language) == Category::Unknown {
                    uncovered.push(format!("{language}:{native}"));
                }
            }
        }
        assert!(
            uncovered.is_empty(),
            "uncategorised real node types: {uncovered:?}"
        );
    }

    #[test]
    fn the_section_collision_is_resolved_by_language() {
        // The bug that validating against real parser output exposed. INI's `[section]` is
        // a mapping; Markdown's `section` is a document section. A global table has to be
        // wrong about one of them.
        assert_eq!(categorize("section", "ini"), Category::Mapping);
        assert_eq!(categorize("section", "toml"), Category::Mapping);
        assert_eq!(categorize("section", "markdown"), Category::Section);
        assert_eq!(categorize("section", "mdx"), Category::Section);
    }

    #[test]
    fn unambiguous_types_do_not_need_a_language() {
        // Most types mean the same thing everywhere; only genuine collisions get an
        // override, so an unknown language still categorises correctly.
        for lang in ["python", "json", "made_up_language"] {
            assert_eq!(categorize("array", lang), Category::Sequence);
            assert_eq!(categorize("pair", lang), Category::KeyValuePair);
        }
    }
}
