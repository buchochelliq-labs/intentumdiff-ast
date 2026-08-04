# intentumdiff-ast

**Canonical cross-language AST (UAST)** — one normalised vocabulary over many grammars, so a
Python `def`, a Java `public static void` and a Rust `fn` are all `FunctionDeclaration`.

```text
source ──tree-sitter grammar──▶ CST ──intentumdiff-ast──▶ UAST ──▶ your tool
```

tree-sitter is the frontend; this is the normalising layer above it.

## Why

A CST is *per grammar*. `if_statement` (Python) and `if_expression` (Rust) are different node
types for the same idea, so every cross-language consumer re-derives the same mapping.

## Design

**Small vocabulary + orthogonal roles.** A lambda is `FunctionDeclaration` + `[Anonymous]`,
not its own category — so "is this a function?" stays one comparison instead of enumerating
four spellings.

```rust
use intentumdiff_ast::{normalize, Category, Role};

let uast = normalize(&cst, "python");
uast.find(Category::Conditional)
    .map(|c| c.has_role(Role::Negated));   // guard clause?
```

**Covers data and documents, not just code.** JSON is mappings and scalars; XML is elements
with attributes; Markdown is headings and paragraphs. None of those are functions, and a
code-only vocabulary has nothing to say about them — yet config, IaC and docs are a large
share of a real diff.

**Configurable token carriage.** Structure is always safe to share; literal source text is
not. `TokenPolicy` is per-category and defaults to carrying *nothing*, so you choose exactly
how much a downstream consumer (an LLM, say) can see:

```rust
TokenPolicy::none()                          // structure only — safe anywhere
TokenPolicy::signatures()                    // names and types, never values
TokenPolicy::all().deny(Category::Literal)   // everything except literal values
```

**No tree-sitter dependency.** The normaliser takes anything implementing `SourceTree`
— `(type, span, children)` — so the vocabulary stays independent of any parser generator,
and usable from a wasm component where a C dependency is not.

## Status

Early. Tier-1 categories are implemented; the vocabulary and roles are tracked as issues.
`Unknown` is a legitimate answer, so a partial vocabulary is honest rather than broken.

See [SPEC.md](SPEC.md) for the schema and design rules.

## Prior art

[Babelfish](https://docs.sourced.tech/babelfish) (GPL-3.0), Semgrep's `AST_generic`
(LGPL-2.1), [srcML](https://www.srcml.org) (GPL-3.0) and
[Codefang](https://sumatoshi-tech.github.io/codefang/architecture/uast/) (Apache-2.0) all
solve this. None was reusable here — three are copyleft, and none is an embeddable Rust
crate — but the designs informed this one. Roles-as-a-dimension comes from Babelfish and
Codefang. The deliberate divergence is `token`: they carry source text unconditionally,
this makes it a policy.

## Licence

MIT
