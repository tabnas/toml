# Tutorial — Parsing TOML with toml (Go)

This lesson walks through parsing a real TOML document from Go. By the
end you will have a program that turns TOML source into a
`map[string]any` and inspects nested values.

No prior Jsonic knowledge is required. You should have Go 1.24 or
later installed.

## 1. Create a module

```sh
mkdir toml-demo
cd toml-demo
go mod init example.com/toml-demo
go get github.com/tabnas/toml/go@latest
```

## 2. Write your first parser

Create `main.go`:

```go
package main

import (
    "fmt"
    toml "github.com/tabnas/toml/go"
)

func main() {
    src := `
title = "TOML Example"

[owner]
name = "Tom Preston-Werner"
dob  = 1979-05-27T07:32:00Z
`
    result, err := toml.Parse(src)
    if err != nil {
        panic(err)
    }
    fmt.Printf("%#v\n", result)
}
```

Run it:

```sh
go run .
```

You should see something equivalent to:

```
map[string]interface {}{
  "title": "TOML Example",
  "owner": map[string]interface {}{
    "name": "Tom Preston-Werner",
    "dob":  (*toml.TomlTime)(0x...),
  },
}
```

## 3. Drill into the result

Because `Parse` returns `any` (an untyped `interface{}`), you'll
normally type-assert on the way down:

```go
m    := result.(map[string]any)
own  := m["owner"].(map[string]any)
name := own["name"].(string)
fmt.Println(name) // Tom Preston-Werner
```

## 4. Inspect a TOML date

TOML distinguishes offset, local, and date-only datetimes. This port
returns them as `*toml.TomlTime` so the distinction survives:

```go
dob := own["dob"].(*toml.TomlTime)
fmt.Println(dob.Kind) // "offset-date-time"
fmt.Println(dob.Src)  // "1979-05-27T07:32:00Z"
```

`Kind` is one of `offset-date-time`, `local-date-time`, `local-date`,
or `local-time`.

## 5. Add structure: tables and arrays

Replace `src` with:

```go
src := `
[database]
server = "192.168.1.1"
ports  = [8001, 8001, 8002]
enabled = true

[[products]]
name = "Hammer"

[[products]]
name = "Nail"
`
```

The same parse call returns nested maps, slices, booleans, and the
array-of-tables as `[]any` of `map[string]any`:

```go
prods := result.(map[string]any)["products"].([]any)
for _, p := range prods {
    fmt.Println(p.(map[string]any)["name"])
}
// Hammer
// Nail
```

## Where to go next

- [How-to guides](./how-to.md) for recipes such as reusing a parser or
  reading from disk.
- [Reference](./reference.md) for the full API and value mapping.
- [Explanation](./explanation.md) for how the Go port works and how it
  differs from the TypeScript port.
