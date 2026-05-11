# toml (Go)

A Go port of [@jsonic/toml](https://github.com/jsonicjs/toml), a
[Jsonic](https://github.com/jsonicjs/jsonic) syntax plugin that parses
TOML into Go `map[string]any` values.

The documentation follows the [Diataxis](https://diataxis.fr) framework
and is split into four purposes:

- [Tutorial](./go/tutorial.md) — a step-by-step lesson for newcomers.
- [How-to guides](./go/how-to.md) — task-oriented recipes for common
  problems.
- [Reference](./go/reference.md) — the exported API, types, and
  value-mapping rules.
- [Explanation](./go/explanation.md) — how the port is structured and
  how it differs from the TypeScript plugin.

## Install

```sh
go get github.com/jsonicjs/toml/go@latest
```

## At a glance

```go
package main

import (
    "fmt"
    toml "github.com/jsonicjs/toml/go"
)

func main() {
    result, err := toml.Parse(`
title = "TOML Example"

[owner]
name = "Tom"
`)
    if err != nil {
        panic(err)
    }
    fmt.Println(result)
}
```

For anything beyond this snippet, start with the
[tutorial](./go/tutorial.md).
