//! Show what a skeleton actually looks like. `cargo run --example render`
use intentumdiff_ast::{normalize, skeleton, SourceTree, Span};

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

fn main() {
    let cases: Vec<(&str, &str, T)> = vec![
        (
            "python",
            "guard clause: if not x: return None; work(x)",
            n(
                "function_definition",
                vec![
                    n("parameters", vec![leaf("identifier")]),
                    n(
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
                    ),
                ],
            ),
        ),
        (
            "python",
            "wrapped body: if x: work()",
            n(
                "function_definition",
                vec![
                    n("parameters", vec![leaf("identifier")]),
                    n(
                        "block",
                        vec![n(
                            "if_statement",
                            vec![leaf("identifier"), n("block", vec![leaf("call")])],
                        )],
                    ),
                ],
            ),
        ),
        (
            "javascript",
            "retry loop with try/catch",
            n(
                "function_declaration",
                vec![n(
                    "statement_block",
                    vec![
                        n(
                            "for_statement",
                            vec![n(
                                "statement_block",
                                vec![n(
                                    "try_statement",
                                    vec![n("statement_block", vec![leaf("call_expression")])],
                                )],
                            )],
                        ),
                        leaf("return_statement"),
                    ],
                )],
            ),
        ),
        (
            "tsql",
            "SELECT with a JOIN and a WHERE",
            n(
                "select_statement",
                vec![
                    leaf("select_clause"),
                    leaf("from_clause"),
                    leaf("join_clause"),
                    leaf("where_clause"),
                ],
            ),
        ),
        (
            "yaml",
            "nested config",
            n(
                "document",
                vec![n(
                    "block_mapping",
                    vec![
                        n("block_mapping_pair", vec![leaf("plain_scalar")]),
                        n(
                            "block_mapping_pair",
                            vec![n("block_sequence", vec![leaf("plain_scalar")])],
                        ),
                    ],
                )],
            ),
        ),
        (
            "plsql",
            "procedure calling out",
            n(
                "create_or_replace_procedure_body",
                vec![n(
                    "plsql_block",
                    vec![
                        n("statement", vec![leaf("call_statement")]),
                        leaf("exception_section"),
                    ],
                )],
            ),
        ),
    ];
    for (lang, desc, tree) in cases {
        let u = normalize(&tree, lang);
        println!(
            "  {:<11} {:<42} {}",
            lang,
            desc,
            skeleton(&u).unwrap_or_else(|| "(none)".into())
        );
    }
}
