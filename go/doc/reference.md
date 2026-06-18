# Reference — Go

Complete, dry description of what `github.com/tabnas/toml/go` exposes, how
TOML values map to Go, and the grammar the port accepts. For a tour see
the [tutorial](tutorial.md); for recipes see the [how-to guide](guide.md);
for the rationale and the TS comparison see [concepts](concepts.md).

## Package

```go
import tabnastoml "github.com/tabnas/toml/go"
```

Runtime dependency: `github.com/tabnas/jsonic/go` (the engine + base
grammar). See `go.mod` for the pinned version.

## Exported API

```go
func Parse(src string, opts ...TomlOptions) (any, error)
func MakeJsonic(opts ...TomlOptions) *tabnasjsonic.Jsonic

const Version = "0.1.2"

type TomlOptions struct{}

type TomlTime struct {
	Kind string // "offset-date-time", "local-date-time", "local-date", "local-time"
	Src  string // original source text
}
```

| Symbol        | Kind     | Purpose                                                |
| ------------- | -------- | ------------------------------------------------------ |
| `Parse`       | function | Parse a TOML source string into `any` (a map).         |
| `MakeJsonic`  | function | Return a configured `*tabnasjsonic.Jsonic` for reuse.        |
| `Version`     | const    | Current module version.                                |
| `TomlOptions` | struct   | Reserved for future options; currently empty.          |
| `TomlTime`    | struct   | Tagged TOML datetime / time value.                     |

## `Parse`

```go
func Parse(src string, opts ...TomlOptions) (any, error)
```

Parses TOML text and returns the root value. For a TOML document the root
is always a `map[string]any`. The `error` is whatever the engine reports
for the first syntax problem; it is `nil` on success.

The common no-options call reuses a single lazily-built, cached engine, so
repeated calls do not rebuild the grammar. Passing a `TomlOptions` builds a
fresh instance for that call.

## `MakeJsonic`

```go
func MakeJsonic(opts ...TomlOptions) *tabnasjsonic.Jsonic
```

Builds and returns a `*tabnasjsonic.Jsonic` with the TOML grammar applied. Use
it to keep a parser around across many calls, or to hand to code that
expects a `*tabnasjsonic.Jsonic`. Call `.Parse(src)` on the returned instance.

## `TomlTime`

```go
type TomlTime struct {
	Kind string
	Src  string
}
```

Returned (as `*TomlTime`) for every TOML datetime or time literal. `Kind`
is derived from the literal's shape; `Src` is the exact source text, kept
so the value can be re-emitted without relying on Go's `time.Time`
formatting. If you need a `time.Time`, parse `Src` yourself.

## Options

`TomlOptions` is an empty struct — there are no user-tunable options. It
exists so the API can grow without a breaking signature change.

## Value mapping

| TOML construct                  | Go result                              |
| ------------------------------- | -------------------------------------- |
| Basic / literal string          | `string`                               |
| Multi-line string (`"""`/`'''`) | `string`                               |
| Integer (`42`, `0xff`, `0o17`, `0b101`, `1_000`) | `float64` (see note) |
| Float (`1.5`, `1e10`)           | `float64`                              |
| `nan`, `+nan`, `-nan`           | `math.NaN()`                           |
| `inf`, `+inf`                   | `math.Inf(+1)`                         |
| `-inf`                          | `math.Inf(-1)`                         |
| Boolean (`true` / `false`)      | `bool`                                 |
| Array `[ … ]`                   | `[]any`                                |
| Inline table `{ … }`            | `map[string]any`                       |
| Table `[a]`                     | `map[string]any`                       |
| Nested table `[a.b]`            | nested `map[string]any` at `a.b`       |
| Array of tables `[[a]]`         | `[]any` of `map[string]any`            |
| Dotted key `a.b = 1`            | nested `map[string]any`                |
| Quoted key `"a b" = 1`          | key `a b` on the enclosing map         |
| Datetime / time literal         | `*TomlTime`                            |
| Line comment `# …`              | discarded                              |

**Number note:** the Go port returns *all* numbers — integers included —
as `float64`. There is no `int64`. Assert numeric leaves as `float64`:

```go
r, _ := tabnastoml.Parse("a = 42")
v := r.(map[string]any)["a"].(float64) // 42
```

## Accepted syntax

The port accepts the core of TOML 1.0:

- **Bare keys**: letters, digits, `_`, `-`.
- **Quoted keys**: `"…"` / `'…'`, including spaces and dots inside the
  quotes (`"a.b" = 1` is a single key named `a.b`).
- **Dotted keys**: `a.b.c = 1` builds nested maps.
- **Strings**: basic `"…"`, literal `'…'`, and the triple-quoted
  multi-line forms `"""…"""` / `'''…'''`. A newline immediately after the
  opening triple delimiter is trimmed.
- **Escape sequences** in basic strings: `\b`, `\t`, `\n`, `\f`, `\r`,
  `\"`, `\\`, `\xHH`, `\uXXXX`, `\UXXXXXXXX`, and the line-ending
  backslash.
- **Integers**: decimal, `0x`, `0o`, `0b`, with `_` separators and a
  leading `+`/`-`.
- **Floats**: decimal point and/or exponent, plus `nan` / `inf` with sign.
- **Booleans**: `true`, `false`.
- **Arrays**: `[ … ]`, mixed element types, optional trailing comma.
- **Inline tables**: `{ key = value, … }`.
- **Tables**: `[name]`, dotted `[a.b.c]`.
- **Array of tables**: `[[name]]`, repeated to append.
- **Comments**: `#` to end of line; the slash and block comment forms are
  disabled.
- **Datetimes / times**: RFC-3339-style date, date-time, and time
  literals — and the same shapes used as keys (`2001-02-03 = 1`,
  `[2002-01-02]`).

## Errors

A malformed document returns a non-nil error. Examples that error: a key
with no value (`a = `), a value with no key (`= 1`), an unterminated
string (`"unterminated`).

## Grammar source and embedding

The canonical grammar is `toml-grammar.jsonic` at the repository root,
written in jsonic syntax. `embed-grammar.js` copies it verbatim into
`go/toml.go` between:

```
// --- BEGIN EMBEDDED toml-grammar.jsonic ---
// --- END EMBEDDED toml-grammar.jsonic ---
```

At load time `apply()` parses that embedded text with a jsonic engine and
installs it. Some grammar pieces the Go engine cannot apply directly are
patched in code: fixed tokens (`=`, `.`), the `#` line-comment definition,
the custom TOML string matcher, the `nan` / `inf` value keywords, the
context-aware date matchers, and an `#ID` lexer guard. See
[concepts](concepts.md) for the details.

Re-run `node embed-grammar.js` after editing the grammar, then rebuild.

## Build and test

| Command                | Purpose                          |
| ---------------------- | -------------------------------- |
| `go build ./...`       | Build the package (run in `go/`).|
| `go test ./...`        | Run the test suite (run in `go/`).|
| `go mod tidy`          | Tidy module dependencies.        |
