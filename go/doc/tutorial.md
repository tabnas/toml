# Tutorial — your first TOML parse (Go)

This walks you from nothing to a working parse. Follow it in order; each
step builds on the last. When you finish you will have installed the
package, parsed a TOML document into a Go `map[string]any`, and looked at
how a TOML date comes back.

You need Go 1.24 or later.

For a recipe-style index of individual tasks, see the
[how-to guide](guide.md). For exhaustive signatures and the full grammar,
see the [reference](reference.md).

## 1. Install

```sh
go get github.com/tabnas/toml/go@latest
```

Import it as `tabnastoml`:

```go
import tabnastoml "github.com/tabnas/toml/go"
```

## 2. Parse a string

`tabnastoml.Parse` takes a TOML source string and returns `(any, error)`. For a
TOML document the value is always a `map[string]any`:

```go
package main

import (
	"fmt"

	tabnastoml "github.com/tabnas/toml/go"
)

func main() {
	result, err := tabnastoml.Parse("a = 1")
	if err != nil {
		panic(err)
	}
	fmt.Println(result) // map[a:1]
}
```

You wrote one key-value pair and got back a map.

## 3. Parse a real document

A `[table]` header opens a nested map; bare `key = value` lines at the top
go on the root map:

```go
package main

import (
	"fmt"

	tabnastoml "github.com/tabnas/toml/go"
)

func main() {
	src := `
title = "TOML Example"

[owner]
name = "Tom"
`
	result, err := tabnastoml.Parse(src)
	if err != nil {
		panic(err)
	}

	m := result.(map[string]any)
	owner := m["owner"].(map[string]any)
	fmt.Println(m["title"])   // TOML Example
	fmt.Println(owner["name"]) // Tom
}
```

Type assertions (`result.(map[string]any)`) are how you walk the tree:
nested tables are `map[string]any`, arrays are `[]any`.

## 4. Add arrays and an array of tables

Arrays use `[ … ]` and become `[]any`; a repeated `[[name]]` header builds
an array of tables:

```go
package main

import (
	"fmt"

	tabnastoml "github.com/tabnas/toml/go"
)

func main() {
	r1, _ := tabnastoml.Parse("a = [1, 2, 3]")
	fmt.Println(r1) // map[a:[1 2 3]]

	src := `
[[products]]
name = "Hammer"

[[products]]
name = "Nail"
`
	r2, _ := tabnastoml.Parse(src)
	m := r2.(map[string]any)
	products := m["products"].([]any)
	first := products[0].(map[string]any)
	fmt.Println(first["name"]) // Hammer
}
```

## 5. Look at a TOML date

TOML's date and time literals come back as a `*tabnastoml.TomlTime`, which keeps
both a `Kind` tag and the original `Src` text:

```go
package main

import (
	"fmt"

	tabnastoml "github.com/tabnas/toml/go"
)

func main() {
	result, _ := tabnastoml.Parse("dob = 1979-05-27T07:32:00Z")
	m := result.(map[string]any)
	dob := m["dob"].(*tabnastoml.TomlTime)

	fmt.Println(dob.Kind) // offset-date-time
	fmt.Println(dob.Src)  // 1979-05-27T07:32:00Z
}
```

`Kind` is one of `offset-date-time`, `local-date-time`, `local-date`, or
`local-time`. If you need a `time.Time`, parse `Src` yourself.

## Where to go next

- [How-to guide](guide.md) — recipes: reuse a parser, handle errors, read
  dates and numbers.
- [Reference](reference.md) — the full API, the value-mapping table, and
  the accepted grammar.
- [Concepts](concepts.md) — how the port works, and how it differs from
  the TypeScript plugin.
