# toml (Go)

A Go port of [@tabnas/toml](https://github.com/tabnas/toml), a
[Jsonic](https://github.com/tabnas/jsonic) syntax plugin that parses
TOML format into Go maps.

## Install

```bash
go get github.com/tabnas/toml/go@latest
```

## Quick example

```go
package main

import (
    "fmt"
    toml "github.com/tabnas/toml/go"
)

func main() {
    result, err := toml.Parse(`
title = "TOML Example"

[owner]
name = "Tom"
dob  = 1979-05-27T07:32:00Z

[[products]]
name = "Hammer"

[[products]]
name = "Nail"
`)
    if err != nil {
        panic(err)
    }
    fmt.Println(result)
}
```

## Documentation

The full documentation lives in [`doc/`](../doc/toml-go.md) and is
organised around the [Diataxis](https://diataxis.fr) framework:

- [Tutorial](../doc/go/tutorial.md)
- [How-to guides](../doc/go/how-to.md)
- [Reference](../doc/go/reference.md)
- [Explanation](../doc/go/explanation.md)

## Features

- Key-value pairs, integers (decimal / `0x` / `0o` / `0b` with `_`
  separators), floats (including `nan`/`inf` with `+/-`), booleans
- Basic and literal strings, triple-quoted multi-line strings, standard
  escape sequences (`\n`, `\t`, `é`, `\U0001F600`, `\xHH`,
  line-ending backslash)
- Arrays, inline tables, tables, nested tables, array-of-tables
- Dotted and quoted keys
- Line comments with `#`
- Datetimes and times returned as `*TomlTime` (kind tag + original
  source)

## License

MIT. Copyright (c) Richard Rodger and other contributors.
