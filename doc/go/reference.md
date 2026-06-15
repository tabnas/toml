# Reference — Go API

Technical description of what `github.com/tabnas/toml/go` exposes and
how TOML values are mapped to Go. For tours and recipes, see the
[tutorial](./tutorial.md) and [how-to guides](./how-to.md).

## Package import

```go
import toml "github.com/tabnas/toml/go"
```

## Exported API

```go
func Parse(src string, opts ...TomlOptions) (any, error)
func MakeJsonic(opts ...TomlOptions) *jsonic.Jsonic

const Version = "0.1.1"

type TomlOptions struct{}
type TomlTime struct {
    Kind string
    Src  string
}
```

| Symbol       | Kind     | Purpose                                                 |
| ------------ | -------- | ------------------------------------------------------- |
| `Parse`      | function | Parse a TOML source string into `any` (a map).          |
| `MakeJsonic` | function | Return a configured `*jsonic.Jsonic` for reuse.         |
| `Version`    | const    | Current module version (bumped by `make publish-go`).   |
| `TomlOptions`| struct   | Reserved for future options; currently empty.           |
| `TomlTime`   | struct   | Tagged TOML datetime value; see below.                  |

## `Parse`

```go
func Parse(src string, opts ...TomlOptions) (any, error)
```

Parses TOML text and returns the root value. For a TOML document the
root is always a `map[string]any`. The `error` is whatever Jsonic
reports for the first syntax problem it finds.

## `MakeJsonic`

```go
func MakeJsonic(opts ...TomlOptions) *jsonic.Jsonic
```

Builds and returns a `*jsonic.Jsonic` with the TOML grammar applied.
Use it when you want to keep a parser around across many calls, or when
you want to pass it to code that expects a `*jsonic.Jsonic`.

## `TomlTime`

```go
type TomlTime struct {
    Kind string // "offset-date-time", "local-date-time", "local-date", "local-time"
    Src  string // original source text
}
```

Returned for every TOML datetime or time literal. `Src` is preserved so
values can be round-tripped back into TOML without relying on Go's
`time.Time` formatting. If you need a `time.Time`, parse `Src` yourself.

## Value mapping

| TOML construct          | Go result                                  |
| ----------------------- | ------------------------------------------ |
| String (basic/literal)  | `string`                                   |
| Multi-line string       | `string`                                   |
| Integer                 | `int64`                                    |
| Float                   | `float64`                                  |
| `nan`, `+nan`, `-nan`   | `math.NaN()`                               |
| `inf`, `+inf`           | `math.Inf(+1)`                             |
| `-inf`                  | `math.Inf(-1)`                             |
| Boolean                 | `bool`                                     |
| Array                   | `[]any`                                    |
| Inline table            | `map[string]any`                           |
| Table `[a]`             | `map[string]any`                           |
| Nested table `[a.b]`    | nested `map[string]any` at `a.b`           |
| Array of tables `[[a]]` | `[]any` of `map[string]any`                |
| Dotted key              | nested `map[string]any`                    |
| Quoted key              | key on the enclosing `map[string]any`      |
| Datetime / time literal | `*TomlTime`                                |
| Line comment `#`        | discarded                                  |

## Integer forms

Decimal, hexadecimal (`0x`), octal (`0o`), binary (`0b`), and
underscore separators (`1_000`) are all accepted.

## Escape sequences in basic strings

`\b`, `\t`, `\n`, `\f`, `\r`, `\"`, `\\`, `\xHH`, `\uXXXX`,
`\UXXXXXXXX`, and the multi-line "line-ending backslash" form are
honoured.

## Grammar embedding

The canonical grammar lives in `toml-grammar.jsonic` at the repository
root. The script `embed-grammar.js` copies it verbatim into
`go/toml.go` between:

```
// --- BEGIN EMBEDDED toml-grammar.jsonic ---
// --- END EMBEDDED toml-grammar.jsonic ---
```

Re-run `node embed-grammar.js` after editing the grammar, then rebuild
(`make build-go`).

## Makefile targets

Relevant Go targets (see [`Makefile`](../../Makefile)):

| Target              | Purpose                                                          |
| ------------------- | ---------------------------------------------------------------- |
| `make build-go`     | `go build ./...` inside `go/`.                                   |
| `make test-go`      | `go test ./...` inside `go/`.                                    |
| `make clean-go`     | `go clean -cache`.                                               |
| `make tidy-go`      | `go mod tidy`.                                                   |
| `make tags-go`      | List existing `go/vX.Y.Z` tags, newest first.                    |
| `make publish-go V=x.y.z` | Bump `Version`, commit, tag, push, create GitHub release.    |
| `make reset`        | Full rebuild of both ports.                                      |

## Runtime dependency

`github.com/tabnas/jsonic/go` (see `go.mod` for the pinned version).
