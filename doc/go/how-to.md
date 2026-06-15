# How-to guides — Go

Short, task-oriented recipes. Each guide assumes:

```go
import toml "github.com/tabnas/toml/go"
```

## Parse a TOML file from disk

```go
b, err := os.ReadFile("config.toml")
if err != nil {
    return err
}
result, err := toml.Parse(string(b))
```

TOML has no streaming model — load the whole file, then parse.

## Reuse a parser across many calls

`toml.Parse` builds a configured Jsonic instance on every invocation.
For hot paths, build it once with `MakeJsonic` and call it directly:

```go
j := toml.MakeJsonic()

for _, src := range inputs {
    result, err := j.Parse(src)
    // ...
}
```

`MakeJsonic` accepts the same `TomlOptions` variadic as `Parse`.

## Type-assert your way down a TOML tree

Because TOML tables can hold heterogeneous values, the result type is
`any`. Assert step by step:

```go
m    := result.(map[string]any)
db   := m["database"].(map[string]any)
host := db["server"].(string)
ports := db["ports"].([]any)
firstPort := int(ports[0].(int64))
```

Integers are `int64`, floats are `float64`, booleans are `bool`, arrays
are `[]any`, and tables are `map[string]any`.

## Distinguish TOML datetime kinds

Every TOML datetime or time literal is returned as `*toml.TomlTime`:

```go
dt := m["dob"].(*toml.TomlTime)
switch dt.Kind {
case "offset-date-time":
    // carries a timezone
case "local-date-time":
    // no zone
case "local-date":
    // date only
case "local-time":
    // time only
}
// dt.Src holds the original source text.
```

Parse `dt.Src` with `time.Parse` (or your preferred library) if you
need a `time.Time`.

## Handle parse errors

`Parse` returns a standard `error`. Inspect its message:

```go
result, err := toml.Parse(src)
if err != nil {
    log.Printf("TOML parse failed: %v", err)
    return err
}
```

## Round-trip special floats

TOML allows `nan`, `+nan`, `-nan`, `inf`, `+inf`, `-inf`. All six parse
into their Go equivalents (`math.NaN()`, `math.Inf(+1)`, `math.Inf(-1)`)
as `float64`. No configuration is required.

## Run the BurntSushi TOML test suite locally

The TypeScript side drives the external `toml-test` suite. For Go, run
the in-package tests that exercise the shared feature matrix:

```sh
make test-go
```

Or directly:

```sh
cd go && go test ./...
```

## Publish a new Go module version

From the repository root:

```sh
make publish-go V=0.1.1
```

This:

1. Bumps `const Version` in `go/toml.go`.
2. Commits, tags `go/v0.1.1`, and pushes.
3. Creates a matching GitHub release when `gh` is available.

## Contribute a grammar fix

1. Edit `toml-grammar.jsonic` at the repository root.
2. Run `node embed-grammar.js` to sync the embedded copies in
   `src/toml.ts` and `go/toml.go`.
3. Run `make test` to exercise both ports.
