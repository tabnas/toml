# Concepts — Go

Background reading: how the Go port works on the engine, the grammar model
it uses, why certain inputs are accepted or rejected, and how it differs
from the TypeScript plugin. For steps see the [tutorial](tutorial.md); for
recipes see the [how-to guide](guide.md); for signatures see the
[reference](reference.md).

## A grammar plugin on the jsonic engine

Like the TypeScript version, the Go port is a TOML **grammar** layered on
the relaxed-JSON jsonic engine — here `github.com/tabnas/jsonic/go`. The
engine supplies the lexer, the rule-driven parser, options, and error
formatting. This package supplies the TOML grammar and a few custom lexer
matchers, plus a small `Parse` / `MakeJsonic` API that installs them.

`MakeJsonic` calls `apply(j)`, which is where all the wiring happens.

## The grammar is shared data

The TOML grammar lives in one file, `toml-grammar.jsonic`, at the
repository root, written in jsonic syntax. The TypeScript build embeds it
into `ts/src/toml.ts`; the same text is embedded into `go/toml.go` between
the `BEGIN/END EMBEDDED` markers. Keeping the grammar as shared data is
what keeps the two runtimes in sync — both are checked against the shared
`test/spec/*.tsv` fixtures.

At load time `apply()` parses the embedded grammar text with a jsonic
engine, then installs it via `j.Grammar(gs)`, resolving the grammar's
`@name` hooks against the map returned by `makeRefs()`.

## Two stages: lexer then parser

The lexer turns source into tokens via matchers that run in priority
order. The parser consumes those tokens according to named rules (`toml`,
`table`, `map`, `pair`, `val`, `elem`, `dive`), each with an open and
close phase made of alternates. The TOML-specific lexer matchers are the
custom string matcher, the `#ID` bare-key token, and the date/time
matchers; the parser rules implement TOML's table/dotted-key descent.

## What the Go port patches in code

The Go jsonic engine cannot apply every part of a grammar-parsed map the
way the TS side does, so `apply()` does several fix-ups after parsing the
embedded grammar — each compensating for a concrete engine difference, not
a behaviour choice:

- **`stripUnsupported`** removes the `@`-ref for the string matcher (the Go
  port installs its own) and re-adds an explicit `#` line-comment
  definition, because Go's `MapToOptions` treats a non-nil `comment.def`
  map as a *full replacement* and would otherwise drop the `#` comment.
- **`registerFixedTokens`** registers `=` (`#CL`) and `.` (`#DOT`)
  explicitly, since Go's `MapToOptions` does not apply `fixed.token` from a
  grammar-parsed map.
- **`injectIDLexGuards`** prepends a never-matching alt with `#ID` at slot
  0 to the close states of `table` and `pair`. Go's lexer only checks alt
  position 0 when deciding whether a custom-regex token like `#ID` is
  expected, while TS checks the position actually being lexed — the guard
  makes `b` in `[b]` lex as `#ID` rather than be rejected.
- **`registerSpecialFloats`** installs `nan` / `inf` (and signed forms) as
  real `float64` values, since these literals cannot round-trip through a
  JSON-ish parse of the grammar text.
- **`registerTomlStringMatcher`** installs a matcher that handles the
  triple-quoted multi-line forms the default jsonic string lexer does not.
- **`registerDateMatchers`** installs context-aware date/time matchers.

## Why date literals need special handling

A value matcher fires unconditionally, so a date-shaped token like
`1979-05-27` would be claimed as a datetime value before the `#ID`
bare-key matcher runs — wrong when the date shape is a *key* (`2001-02-03 =
1`, the table header `[2002-01-02]`, or `a.2001-02-08 = 7`).

`makeDateMatcher` resolves this with `isKeyContext`: it scans the current
rule's alternates for `#ID`. Key-accepting rules (`toml`, `map`, `dive`,
`pair`, `table`) list `#ID`; value-producing rules (`val`, `list`, `elem`)
do not. In a key context the matcher emits an `#ID` token; otherwise it
emits a `#VL` token carrying a `*TomlTime`.

## The grammar model: tables and dives

TOML's shape is unusual: a value's position in the document determines its
position in the tree. A `[a.b]` header walks or creates the path `a → b`
and routes following pairs into it; `[[a]]` appends a fresh map to an array
at `a`; a dotted key `a.b.c = 1` descends one map per segment. The
`table` and `dive` rules drive this with counter-guarded alternates
(`table_dive`, `table_array`, `dive_key`) and the `@table-*` / `@dive-*`
actions in `refs.go` that build or reuse the nested maps and slices.

One Go-specific wrinkle: Go slice headers are not shared through map
values, so when the table machinery grows an array it writes the new slice
back to its home map (tracked via the `arr_parent` / `arr_key` user
fields). The TS port mutates a JavaScript array in place and needs no such
write-back.

## Accepted vs rejected

Accepted (all covered by the test suites): bare, quoted, and dotted keys
(including `"a.b" = 1` as a single key); integers in all bases with `_`
separators; floats with exponents and `nan` / `inf`; basic, literal, and
triple-quoted strings with the full escape set; arrays, inline tables,
tables, nested tables, array-of-tables; `#` comments; and the four
date/time shapes — including as keys.

Rejected (returns a non-nil error): a key with no value (`a = `); a value
with no key (`= 1`); an unterminated string (`"unterminated`).

## Differences from the TS version

The two ports parse the same grammar and pass the same shared fixtures,
but the language shapes differ:

| Aspect            | TypeScript                               | Go                                        |
| ----------------- | ---------------------------------------- | ----------------------------------------- |
| Entry point       | `new Tabnas().use(jsonic).use(Toml)` then `.parse(src)` | `toml.Parse(src)` or `toml.MakeJsonic().Parse(src)` |
| Root value        | plain `object`                           | `map[string]any`                          |
| Object / table    | plain `object`                           | `map[string]any`                          |
| Array             | `Array`                                  | `[]any`                                   |
| **Integer**       | `number`                                 | **`float64`** (no `int64`)                |
| Float             | `number`                                 | `float64`                                 |
| `nan` / `inf`     | `NaN` / `Infinity` / `-Infinity`         | `math.NaN()` / `math.Inf(±1)`             |
| Datetime / time   | `Date` with a `__toml__` tag             | `*TomlTime` (`Kind` + `Src` fields)       |
| Error reporting   | thrown; `err.code === 'unexpected'`      | returned `error` value                    |
| Options type      | `TomlOptions = {}`                       | `TomlOptions struct{}`                    |

Implementation differences (no behavioural effect on parse results):

- The Go port keeps the grammar's *regex-based* date/time value matchers in
  the embedded text but never reaches them — the context-aware matchers
  always run first. The TS port replaces them outright after install.
- The Go port adds the in-code patches listed above
  (`injectIDLexGuards`, `registerFixedTokens`, the `#` comment re-add,
  array write-back) to compensate for differences in the Go engine's
  option-mapping and lexer; the TS engine needs none of these.
- `Parse` caches a single shared engine for the no-options path; the TS
  side expects you to build one `Tabnas` instance and reuse it yourself.

## Design rationale

- **One grammar, two runtimes.** Authoring the grammar once as data and
  embedding it into both ports makes behaviour match by construction.
- **Reuse the engine.** Building on jsonic gives TOML the engine's lexer,
  error reporting, and rule machinery; the port only expresses what is
  different about TOML.
- **Preserve source for dates.** `time.Time` formatting is lossy, so the
  original text is kept in `TomlTime.Src` for faithful round-tripping.
