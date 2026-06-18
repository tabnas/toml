# How-to guide — Go

Task-oriented recipes. Each is self-contained. For a guided introduction
see the [tutorial](tutorial.md); for complete signatures and the grammar
see the [reference](reference.md).

Every recipe assumes the package is installed:

```sh
go get github.com/tabnas/toml/go@latest
```

and imported as:

```go
import toml "github.com/tabnas/toml/go"
```

## Parse a string

`toml.Parse` returns `(any, error)`. The root of a TOML document is always
a `map[string]any`:

```go
result, err := toml.Parse("a = 1\nb = 2")
if err != nil {
	// handle
}
m := result.(map[string]any) // map[a:1 b:2]
```

The common no-options call reuses a single cached engine, so repeated
`toml.Parse` calls do not rebuild the grammar each time.

## Keep a parser for many parses

When you want an explicit, reusable instance — for example to pass to code
that expects a `*jsonic.Jsonic` — build one with `MakeJsonic` and call its
`Parse`:

```go
j := toml.MakeJsonic()
r1, _ := j.Parse("port = 8080")
r2, _ := j.Parse(`host = "db"`)
```

Building the instance installs the whole TOML grammar (the expensive
step); parsing only reads instance state, so a shared `*jsonic.Jsonic` is
safe to reuse.

## Parse a file from disk

`Parse` takes a string, so read the file first:

```go
data, err := os.ReadFile("config.toml")
if err != nil {
	// handle
}
result, err := toml.Parse(string(data))
```

## Handle a parse error

A syntax error is returned as the second value. Malformed input — a key
with no value, a value with no key, an unterminated string — yields a
non-nil `error`:

```go
if _, err := toml.Parse("a = "); err != nil {
	fmt.Println(err) // formatted diagnostic pointing at the bad position
}
```

The error message is a multi-line diagnostic with a caret under the
offending column; print it as-is for a readable report.

## Walk the result tree

Nested tables are `map[string]any`; arrays are `[]any`. Assert as you
descend:

```go
result, _ := toml.Parse(`
[db]
host = "localhost"
port = 5432
`)
m := result.(map[string]any)
db := m["db"].(map[string]any)
host := db["host"].(string) // "localhost"
port := db["port"].(float64) // 5432
```

Note that **all numbers are `float64`**, including integers — see the
reference and the concepts page for why.

## Read integers in every base

Decimal, hex (`0x`), octal (`0o`), and binary (`0b`) all parse to the same
`float64`; `_` digit separators are allowed:

```go
r, _ := toml.Parse("a = 0xff")
m := r.(map[string]any)
fmt.Println(m["a"]) // 255

r2, _ := toml.Parse("a = 1_000")
fmt.Println(r2.(map[string]any)["a"]) // 1000
```

`nan`, `inf`, and their signed forms parse to `math.NaN()`,
`math.Inf(+1)`, and `math.Inf(-1)`:

```go
r, _ := toml.Parse("a = -inf")
v := r.(map[string]any)["a"].(float64)
fmt.Println(math.IsInf(v, -1)) // true
```

## Read a date or time and tell its kind

Date and time literals come back as a `*toml.TomlTime`. Use `Kind` to
distinguish the four TOML shapes; `Src` is the original text:

```go
parse := func(s string) string {
	r, _ := toml.Parse("a = " + s)
	return r.(map[string]any)["a"].(*toml.TomlTime).Kind
}

parse("1979-05-27")          // "local-date"
parse("1979-05-27T07:32:00") // "local-date-time"
parse("1979-05-27T07:32:00Z")// "offset-date-time"
parse("07:32:00")            // "local-time"
```

## Use multi-line and literal strings

Basic strings (`"…"`) honour escapes; literal strings (`'…'`) are verbatim;
the triple-quoted forms span lines:

```go
r1, _ := toml.Parse(`a = """hello"""`)
fmt.Println(r1.(map[string]any)["a"]) // hello

r2, _ := toml.Parse("a = \"\"\"line1\nline2\"\"\"")
fmt.Println(r2.(map[string]any)["a"]) // line1\nline2
```
