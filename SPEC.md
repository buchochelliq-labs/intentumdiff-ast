# UAST Schema v0

The canonical vocabulary. One normalised category for many language-specific node types, so
a Python `def`, a Java `public static void` and a Rust `fn` are all `FunctionDeclaration`.

## Why a canonical AST at all

A tree-sitter CST is *per grammar*. `if_statement` in Python and `if_expression` in Rust are
different node types for the same idea, and every consumer that wants to reason across
languages ends up re-deriving the mapping. IntentumDiff derived it three separate times —
in the diff engine, in the fact predicates, and again in the shape skeleton — which is the
signal that it wanted to be a layer.

Prior art solved this the same way. [Babelfish](https://docs.sourced.tech/babelfish)
(UAST, GPL-3.0), [Semgrep](https://semgrep.dev)'s `AST_generic` (LGPL-2.1) and
[srcML](https://www.srcml.org) (GPL-3.0) all normalise to a shared vocabulary. None is
reusable here — Semgrep is OCaml, Babelfish is Go, srcML is C++ and covers four languages,
and two of the three are copyleft, which an MIT open-core product cannot take on. The
*design* is worth borrowing even where the code is not.

## Design rules

1. **Category, not syntax.** A node's category says what it *means*, never how it was
   spelled. `unless x` and `if not x` both normalise to a conditional with a negated test.
2. **Structure is preserved, content is not.** Node kinds and nesting are carried; names and
   literals are NOT part of a category. Where a consumer needs identifiers it reads them
   from the node it came from, so a UAST alone is safe to send somewhere a raw AST is not.
3. **Lossy is allowed, lying is not.** A construct with no category is `Unknown` and keeps
   its native type. Never force a bad fit — a wrong category is worse than an honest gap,
   because downstream reasoning cannot tell it was a guess.
4. **Additive versioning.** Categories are only ever added. Consumers must ignore unknown
   categories rather than fail, so a newer producer cannot break an older reader.
5. **Wrappers collapse.** `block`, `body`, `expression_statement` and similar grammar
   scaffolding carry no category; their children splice into the parent. This is what makes
   the same code produce the same UAST across grammars and across reformatting.

## The vocabulary

47 categories in eight groups. `TIER` marks the first tranche — the minimum that makes
structural intent legible.

### Declarations
| Category | Tier | Covers |
|---|---|---|
| `FunctionDeclaration` | **1** | function, method, lambda, closure, procedure |
| `ClassDeclaration` | **1** | class, struct, record, object, trait, interface |
| `VariableDeclaration` | 2 | let/var/const binding with an initialiser |
| `ParameterDeclaration` | 2 | one formal parameter |
| `FieldDeclaration` | 3 | class/struct member |
| `TypeDeclaration` | 3 | typedef, type alias, generic parameter |
| `EnumDeclaration` | 3 | enum and its variants |
| `ModuleDeclaration` | 3 | module, namespace, package |
| `ImportDeclaration` | 2 | import, use, require, include |
| `ExportDeclaration` | 3 | export, public re-export |

### Control flow
| Category | Tier | Covers |
|---|---|---|
| `Conditional` | **1** | if/elif/else, ternary, guard, unless |
| `Loop` | **1** | for, while, do-while, repeat, comprehension |
| `Switch` | 2 | switch, match, case, when |
| `Return` | **1** | return, implicit tail return |
| `Break` | 3 | break, last |
| `Continue` | 3 | continue, next |
| `Goto` | 3 | goto, labelled jump |
| `Yield` | 2 | yield, yield from |
| `Await` | 2 | await, .await |

### Error handling
| Category | Tier | Covers |
|---|---|---|
| `TryBlock` | **1** | try, begin/rescue, do/catch |
| `CatchClause` | 2 | catch, except, rescue |
| `FinallyClause` | 2 | finally, ensure |
| `Throw` | 2 | throw, raise, panic |
| `Assertion` | 3 | assert, precondition |

### Operations
| Category | Tier | Covers |
|---|---|---|
| `Call` | **1** | function/method/constructor invocation |
| `Assignment` | **1** | =, +=, destructuring bind, mutation |
| `UnaryOperation` | 2 | not, negate, dereference |
| `BinaryOperation` | 2 | arithmetic, concatenation |
| `Comparison` | 2 | ==, !=, <, >, is, in |
| `LogicalOperation` | 2 | and, or, xor, short-circuit |
| `IndexAccess` | 3 | a[i], slice |
| `MemberAccess` | 3 | a.b, a->b, a::b |
| `Cast` | 3 | type cast, coercion |

### Literals
| Category | Tier | Covers |
|---|---|---|
| `StringLiteral` | 2 | string, char, template, heredoc |
| `NumericLiteral` | 2 | int, float, hex, decimal |
| `BooleanLiteral` | 2 | true/false |
| `NullLiteral` | 2 | null, nil, None, undefined |
| `CollectionLiteral` | 2 | list, array, tuple, set |
| `MapLiteral` | 2 | dict, object, map, record |

### Structured data
| Category | Tier | Covers |
|---|---|---|
| `KeyValuePair` | 2 | mapping entry (YAML/JSON/TOML/INI) |
| `Mapping` | 2 | mapping container |
| `Sequence` | 2 | ordered collection |
| `ResourceBlock` | 2 | declarative block (HCL, K8s, CFN) |
| `Attribute` | 2 | attribute inside a resource block |

### References
| Category | Tier | Covers |
|---|---|---|
| `Identifier` | 2 | name reference |
| `TypeReference` | 3 | a type used, not declared |
| `Annotation` | 3 | decorator, attribute, annotation |

### Other
| Category | Tier | Covers |
|---|---|---|
| `Comment` | 3 | comment, docstring |
| `Unknown` | **1** | no category applies; native type retained |

## Node shape

```jsonc
{
  "category": "Conditional",
  "native_type": "if_statement",   // provenance: what the grammar called it
  "language": "python",
  "children": [ /* UastNode */ ],
  "props": { "negated": true }     // category-specific, structural only
}
```

`native_type` is kept deliberately: Babelfish's most-copied decision was retaining the
native AST *alongside* the universal one, so a consumer that needs grammar detail is never
forced back to re-parsing.

## Stability

Two files that differ only by formatting MUST produce identical UAST. That is the property
every consumer relies on — it is what makes a UAST diff mean "the code changed" rather than
"the file moved". It is also why wrapper collapse is a rule and not an optimisation.
