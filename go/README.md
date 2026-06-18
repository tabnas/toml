# toml (Go)

A Go port of [@tabnas/toml](https://github.com/tabnas/toml), a
[Jsonic](https://github.com/tabnas/jsonic) grammar plugin that parses
[TOML](https://toml.io) into Go `map[string]any` values.

## Install

```sh
go get github.com/tabnas/toml/go@latest
```

## Example

```go
package main

import (
	"fmt"

	tabnastoml "github.com/tabnas/toml/go"
)

func main() {
	result, err := tabnastoml.Parse(`
title = "TOML Example"

[owner]
name = "Tom"

[[products]]
name = "Hammer"

[[products]]
name = "Nail"
`)
	if err != nil {
		panic(err)
	}
	fmt.Println(result)
	// map[owner:map[name:Tom] products:[map[name:Hammer] map[name:Nail]] title:TOML Example]
}
```

`tabnastoml.Parse` returns `(any, error)`; the root is a `map[string]any`.
Numbers (integers included) come back as `float64`, and datetime/time
literals as `*tabnastoml.TomlTime`.

## Documentation

Organised around the [Diataxis](https://diataxis.fr) framework:

- [Tutorial](doc/tutorial.md) — a step-by-step first parse.
- [How-to guide](doc/guide.md) — task-oriented recipes.
- [Reference](doc/reference.md) — the exported API, value mapping, and
  accepted grammar.
- [Concepts](doc/concepts.md) — how the port works, including a
  "Differences from the TS version" section.

## Features

- Key-value pairs; integers (decimal / `0x` / `0o` / `0b` with `_`
  separators); floats (including `nan` / `inf` with `+`/`-`); booleans.
- Basic, literal, and triple-quoted multi-line strings with the standard
  escape set (`\n`, `\t`, `\xHH`, `\uXXXX`, `\UXXXXXXXX`, line-ending
  backslash).
- Arrays, inline tables, tables, nested tables, array-of-tables.
- Dotted and quoted keys.
- `#` line comments.
- Datetimes and times as `*TomlTime` (kind tag + original source).

## License

MIT. Copyright (c) Richard Rodger and other contributors.
