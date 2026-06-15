# Explanation — Go design

Background and rationale for readers who want to understand how the Go
port of `@tabnas/toml` is built and where it diverges from the
TypeScript plugin. For task-focused material, go to the
[how-to guides](./how-to.md) and [reference](./reference.md).

## Why reuse the Jsonic grammar engine?

The TypeScript plugin treats TOML as "JSON with extra rules": it layers
TOML-specific grammar onto Jsonic rather than writing a bespoke parser.
The Go port preserves that design, so both ports:

- Share one declarative grammar file.
- Exhibit the same parse behaviour for the same input.
- Are maintained together — a grammar change in one is a grammar change
  in both.

The Go implementation depends on
[`github.com/tabnas/jsonic/go`](https://github.com/tabnas/jsonic),
which is the Go port of Jsonic.

## Single source of truth for the grammar

`toml-grammar.jsonic` at the repository root is the canonical grammar.
The Node.js script `embed-grammar.js` copies it verbatim into two
ports:

- `src/toml.ts` (TypeScript).
- `go/toml.go` (Go).

Both ports therefore share one grammar. If you only ever edit the
embedded `const grammarText = ...` block in a source file, your change
will be overwritten. Always edit `toml-grammar.jsonic`.

## How the port hooks Jsonic

At load time the Go code:

1. Parses the embedded grammar using a plain Jsonic instance.
2. Installs it via `jsonic.Grammar()` with a `refs` map whose entries
   resolve `@<name>` placeholders to Go functions.
3. Registers a custom string matcher for TOML's basic, literal, and
   triple-quoted multi-line forms.
4. Adds `true`, `false`, `null`, `nan`, `inf` value definitions
   (including `+/-` prefixes). `SetOptions(Value.Def)` otherwise
   replaces these wholesale, so they have to be reapplied after grammar
   installation.
5. Registers value matchers for ISO dates and local times, returning
   `*TomlTime`.

## Go-specific patches

Two quirks in the Go port of Jsonic need workarounds that don't apply
to the TypeScript side:

- **Fixed-token options.** `options.fixed.token` entries are applied
  explicitly — `MapToOptions` in Go-jsonic does not.
- **Default comment defs.** `comment.def` with null-only entries would
  wipe the default `#` line comment, so it is re-added.
- **Lexer slot expectations.** Never-matching dummy alts with `#ID` at
  slot 0 are injected into `table:close` and `pair:close` so the
  lexer's `matchMatch` treats `#ID` as an expected token at slot 1 as
  well.

These are implementation details that a user should never have to see;
they live in `go/toml.go` and `go/refs.go` and are documented there in
comments.

## Datetimes as `*TomlTime`, not `time.Time`

TOML has four datetime variants: offset-datetime, local-datetime,
local-date, and local-time. Go's `time.Time` has a single representation
and always carries a location; flattening TOML onto it would lose the
distinction between "no zone" and "UTC".

`*TomlTime` keeps both the `Kind` tag and the original `Src` text. When
you need arithmetic, re-parse `Src` with `time.Parse`. When you only
need to round-trip or preserve intent, use `Src` directly.

The TypeScript port makes the same trade-off with a `__toml__` sidecar
on the built-in `Date` object.

## Scope and non-goals

- Targets the TOML 1.0 feature set.
- Parse only — there is no TOML writer.
- No configurable options today. `TomlOptions` exists for
  forward-compatibility (variadic argument on `Parse` / `MakeJsonic`).

## Relationship to the TypeScript plugin

The Go port is a sibling, not a dependency. They share the grammar,
feature matrix, and test fixtures (via `toml_tsv_test.go` and the
BurntSushi `toml-test` suite). Behavioural differences are treated as
bugs; file them with reproduction input that fails in only one port.
