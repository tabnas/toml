# Agents Guide: rs/

The Rust port of the canonical TypeScript in [`../ts`](../ts). Read
[`../AGENTS.md`](../AGENTS.md) first: it holds the cross-runtime rules,
the grammar-embed rule and the conformance-suite rules, and this file
only covers what is specific to this crate.

## Layout

| Path | |
|---|---|
| `src/lib.rs` | the embedded grammar, the document adjustments, `toml`, `plugin`, `make`, `make_with`, `parse`, `VERSION` |
| `src/refs.rs` | every `@`-named reference the grammar uses: state actions, alternate actions, conditions, conditional `p:`/`r:` targets |
| `src/node.rs` | table nodes as PATHS (see below), and the key-conflict diagnosis |
| `src/strmatcher.rs` | TOML's basic, literal and multi-line strings |
| `src/datematcher.rs` | the context-aware date and time matchers, and the leading-BOM matcher |
| `src/daterange.rs` | whether a date or time whose SHAPE matched denotes a real instant |
| `src/values.rs` | `TomlTime`, and how it rides on a `tabnas::Value` |
| `tests/parity_test.rs` | every `../test/spec/*.tsv` fixture, discovered by listing |
| `tests/toml_test.rs` | in-language behaviour: API, special floats, triple quotes, date kinds, BOM, error columns, key conflicts, the embedded grammar, threads |
| `tests/toml_valid_test.rs` | the BurntSushi/toml-test corpus, both halves |
| `tests/divergent_test.rs` | the divergence register, `rust` column |
| `tests/perf_test.rs` | `parse` reuses its instance |
| `tests/version_test.rs` | Cargo.toml == `VERSION` == ts/package.json |
| `tests/common/mod.rs` | shared helpers: spec dir, repo dir, value and failure conversion |
| `README.md` | the crate front page, prose-gated; its `rust` fences are doctests of this crate |

Crate `tabnas-toml`, library `tabnas_toml`. The engine (`tabnas`), the
jsonic core (`tabnas-jsonic`) and the fixture runner (`tabnas-support`,
dev only) are **path dependencies on sibling checkouts**
(`../../parser/rs`, `../../jsonic/rs`, `../../support/rs`). None is
published, so there is no registry version to fall back on.

```bash
cargo build --all-targets
cargo test --all-targets && cargo test --doc
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

`make test-rs` from the repository root is the fast loop; `ci/rust/run.sh`
is the full gate and adds `fmt --check`, the lockfile check and the MSRV
pin.

## THE BIG ONE: a table node is a PATH, not a reference

Read this before touching `refs.rs` or `node.rs`.

The canonical grammar hands a table rule a REFERENCE to a nested object
(`r.node = r.parent.node[key]`) and then lets that table's pairs be
written straight into it. JavaScript objects alias; so does Go's
`*OrderedMap`. A `tabnas::Value` does not: its containers are
`Arc<IndexMap>` and `Arc<Vec>`, mutated through `Arc::make_mut`, which
COPIES as soon as a second handle exists. A clone of a nested map is a
snapshot, and writes to it never reach the document.

So here a node is a CELL plus a PATH: the `Rc<RefCell<Value>>` the rule
already carries, and the list of keys and indices from that cell's value
down to the table. Every read walks the path from the cell and every
write walks it again, so a write lands in the real tree however the
`Arc`s happen to be shared. The path lives in the rule's `u` bag under
`node::PATH_KEY`, because `u` is per-rule and merged key by key, so an
alternate's own `u: { ... }` does not disturb it.

The translation is mechanical, and each canonical line has exactly one
form here:

| canonical | here |
|---|---|
| `r.node = r.parent.node` | `set_path(rule, &parent_path(rule))` |
| `r.node = tableAt(r.parent.node, key, …)` | `set_path(rule, &table_at(&cell, &parent_path(rule), key, …)?)` |
| `Array.isArray(r.prev.node)` | `node::is_list(&cell, &prev_path(rule))` |
| `Object.assign(r.node, r.child.node)` | `merge_into(&cell, &path_of(rule), &child_value)` |
| `r.node[key] = r.child.node` | `node::write(&cell, &(path + key), child_value)` |
| `r.prev.node.push(node())` | `node::push(&cell, &prev_path(rule), new_map())` |

Two rules follow from it:

- **`node::write` creates only its FINAL segment.** Every caller builds
  the parent first, through `table_at` or `array_at`. A write whose
  parent does not exist is a silent no-op, which is what the first cut of
  this got wrong: `[a]` produced `{}` because `write(cell, ["a"], …)`
  navigated to a key that was not there yet.
- **Never hold a cloned sub-value across a write.** Reads clone, and a
  clone bumps the `Arc`, so a write while one is alive copies a container
  it did not need to.

## Two lifecycle actions that are not where the canonical ones are

**`@dive-bo` has no canonical counterpart.** There, the engine's own node
inheritance says what a pushed or replaced `dive` rule's node is: the
parent's when it was pushed, the previous rule's when it was replaced. A
path has to be inherited explicitly to say the same thing, and
`rule.prev_rule.is_some()` is exactly "this rule was created by a
replace" (the engine sets `prev_rule` only on that path).

**`@table-ac` is done in `@table-bc`.** The canonical handler writes
`next.n.table_dive = 0` on the rule the parser moves to next. Here `next`
arrives as an immutable `&RuleSnapshot`, and even a mutable one would not
help: the engine builds the replacement rule's counters with
`Rc::clone(&current.n)`, so a `make_mut` in the after phase would split
the two rather than reach the next rule. The reset is therefore made on
THIS rule in the BEFORE-close phase, which the engine runs before it
clones the counters, so the next rule gets them. No table close alternate
reads either counter, so nothing between the two phases can see the
difference. The test that this still works is the `array-of-tables.tsv`
fixture: without it, a second `[[a]]` never starts a new element.

## What the grammar document needs before it is installed

`grammar_document` parses `../toml-grammar.jsonic` with a standard jsonic
instance, exactly as the other two ports do, and then makes four
adjustments. Each is in `lib.rs` next to its reason; the short version:

1. **`integerize`.** Every number in a jsonic parse result is an `f64`,
   so `b: 2` arrives as `2.0` and the grammar loader refuses it: a
   backtrack count must be a non-negative integer. Go does the same thing
   one field at a time in `mapToAlt`.
2. **`adjust_token_sets`.** The canonical token-set form pads a set to
   four slots with nulls; this loader takes the members alone and rejects
   a null entry.
3. **`adjust_lex_matchers`.** The grammar's `lex.match.string.make` has
   no `order`, because in the canonical engine the string matcher is a
   named builtin being REPLACED. Here the builtin bands are fixed and a
   custom matcher is placed among them by order, so one is supplied. The
   BOM and date matchers are added here rather than to the shared grammar
   text, because that text is read by two ports that install their own.
4. **`adjust_messages`.** The `error` and `hint` templates, kept in step
   with the TypeScript registration word for word.

`register_special_floats` runs AFTER the document, because a keyword
value definition takes a literal `val` and never a function reference, so
there is no `@`-name for a number JSON cannot spell. Both other ports
patch `nan` and `inf` in code for the same reason.

## What this port does NOT need

- **No `injectIDLexGuards`.** The Go port prepends a never-matching
  alternate with `#ID` at slot 0, because its `matchMatch` only consults
  slot 0 when deciding whether a custom-regex token is expected. This
  engine's `expected_match_tins` is per-slot, as TypeScript's is, so `b`
  in `[b]` lexes as `#ID` with no help.
- **No `stripUnsupported` for the string matcher.** The Go port removes
  `options.lex.match.string` because it has no way to resolve the
  reference; here `lex_match_factory_ref` resolves it, so the grammar's
  own declaration is what installs the matcher.
- **No comment-definition repair.** This loader treats
  `comment.def.slash: null` as a removal, as TypeScript does, so `#`
  survives. The Go loader treats a non-nil `comment.def` as a full
  replacement and has to re-add `hash`.

## A string token's point is its END

`strmatcher` builds its `#ST` token from the cursor AFTER the string, not
before it. That reads like a bug and is the canonical behaviour: the
TypeScript matcher writes its finished `sI`/`rI`/`cI` back into `pnt` and
only then builds the token, and the Go matcher does the same. So a
diagnostic about the token after a quoted key points at the end of the
key, which is what `go/strmatcher_col_test.go` and the TypeScript 'error
columns count characters, not bytes' test both measure. Building the
token from the point captured on entry moves every one of those columns
back to the opening quote, and `string_error_columns_count_scalars_not_bytes`
in `toml_test.rs` is what says so.

The scan itself hands the engine a COUNT of scalars and lets
`advance_chars` do the row and column arithmetic. The Go port walks bytes
and owns that arithmetic itself, which is why it needs a test for it;
here there is no second implementation of it to drift.

## The lax hex scan in `\u` is deliberate

`is_hexadecimal_lax` accepts every ASCII letter, not only `a`-`f`,
because the canonical `isHexadecimal` does. The range check below it is
what rejects `\uZZZZ`, and the digits a lax scan lets through are then
read by `hex_prefix`, which stops at the first non-hex character exactly
as `parseInt` does. Narrowing it to real hex digits would change which
documents parse: `"\u12g4"` is U+0012 in TypeScript and here, and
`invalid_unicode` in Go. That is a TypeScript/Go disagreement this port
did not create and does not adjudicate; it reproduces TypeScript, which
is the rule. It is not in `../test/divergent.tsv` because measuring the
two cells there means running those two ports, and nothing here can.

## The divergence register has a local runner

`tests/divergent_test.rs` carries its own comparator rather than using
`tabnas_support::Register`, and so do the other two halves. That library
compares two cells by error CODE and drops the `@row:col` suffix; every
row of this register is a positional disagreement on the same code, so it
reads every row as "records no divergence" and the file asserts nothing.
`same_expectation_reads_the_position` pins the comparator, because a
comparator that stops distinguishing positions does not fail, it just
makes the register vacuous. When `tabnas_support` compares positions,
delete the local comparator in all three halves together.

## The conformance suite never skips

`tests/toml_valid_test.rs` runs `../scripts/fetch-toml-test.sh` itself
when the corpus is missing and FAILS if it still is. Do not reintroduce a
skip: both older suites used to skip when the corpus was absent, which is
exactly what CI looked like, so neither had ever executed there while the
job reported green.

The `rust` row of `../test/conformance.tsv` is EXACT, not a floor, for
the reason that file's header gives. It reproduces the `ts` row.

## The docs are gated

`README.md` is in the published set: no em dashes in prose, no first
person singular, no links to any `AGENTS.md`, no project history. This
file is internal and may be blunt.

## The README is doctested

`src/lib.rs` includes `README.md` as rustdoc under `#[cfg(doctest)]`, so
every `rust` fence in it runs on `cargo test --doc` (they show up as
`readme_examples (line N)`). rustdoc runs each fence as written, so a
fence must be a complete program: wrap it in
`fn main() -> Result<(), Box<dyn std::error::Error>> { ... Ok(()) }`
rather than using `?` at the top level, and never use hidden `# ` lines,
which render as garbage on GitHub. The `toml` and `bash` fences are not
run.
