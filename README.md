# @tabnas/toml

<!-- tabnas-badges -->
[![npm](https://tabnas.github.io/status/badges/toml-npm.svg)](https://www.npmjs.com/package/@tabnas/toml)
[![CI](https://github.com/tabnas/toml/actions/workflows/ci.yml/badge.svg)](https://github.com/tabnas/toml/actions/workflows/ci.yml)
[![go](https://tabnas.github.io/status/badges/toml-go.svg)](https://pkg.go.dev/github.com/tabnas/toml/go)
[![tabnas standard](https://tabnas.github.io/status/badges/toml-standard.svg)](https://tabnas.github.io/status/)
<!-- /tabnas-badges -->

A [TOML](https://toml.io) parser built as a grammar plugin on the
[tabnas](https://github.com/tabnas/parser) engine and the
[jsonic](https://github.com/tabnas/jsonic) relaxed-JSON grammar. One
grammar, three runtimes: a TypeScript/JavaScript plugin, a Go port and a
Rust port that parse the same syntax into native objects, maps and
values.

Docs, guides, the error reference and the playground: **[tabnas.dev](https://tabnas.dev)**.

| Path | Description |
|---|---|
| [`ts/`](ts/) | TypeScript / JavaScript implementation (`@tabnas/toml`). |
| [`go/`](go/) | Go port (`github.com/tabnas/toml/go`). |
| [`rs/`](rs/) | Rust port (crate `tabnas-toml`, library `tabnas_toml`). |
| [`toml-grammar.jsonic`](toml-grammar.jsonic) | The single shared grammar, embedded into all three. |
| [`test/spec/`](test/spec/) | Shared conformance fixtures, run by every runtime. |

## Install

TypeScript / JavaScript:

```sh
npm install @tabnas/toml @tabnas/parser @tabnas/jsonic
```

Go:

```sh
go get github.com/tabnas/toml/go@latest
```

Rust: the crate is not published, because it takes the engine and the
jsonic core as path dependencies. Clone
`https://github.com/tabnas/parser` and `https://github.com/tabnas/jsonic`
beside this repository and point at all three:

```toml
[dependencies]
tabnas-toml = { path = "../toml/rs" }
tabnas-jsonic = { path = "../jsonic/rs" }
tabnas = { path = "../parser/rs" }
```

## One tiny example

TypeScript / JavaScript:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('title = "TOML Example"\n[owner]\nname = "Tom"')
// => { title: 'TOML Example', owner: { name: 'Tom' } }

// One expression, verified:
toml.parse('a = 1\nb = [2, 3]')   // => { a: 1, b: [2, 3] }
```

Go:

```go
import tabnastoml "github.com/tabnas/toml/go"

result, err := tabnastoml.Parse(`
title = "TOML Example"
[owner]
name = "Tom"
`)
// result == map[string]any{"title": "TOML Example", "owner": map[string]any{"name": "Tom"}}
```

Rust:

```rust
let value = tabnas_toml::parse("title = \"TOML Example\"\n[owner]\nname = \"Tom\"")?;
// value.to_string() == r#"{"title":"TOML Example","owner":{"name":"Tom"}}"#
```

## Documentation

The docs follow the [Diataxis](https://diataxis.fr) framework: one file
per purpose, per language:

| Purpose       | TypeScript | Go |
|---------------|-----------|----|
| Tutorial (learn) | [ts/doc/tutorial.md](ts/doc/tutorial.md) | [go/doc/tutorial.md](go/doc/tutorial.md) |
| How-to (recipes) | [ts/doc/guide.md](ts/doc/guide.md) | [go/doc/guide.md](go/doc/guide.md) |
| Reference (API + syntax) | [ts/doc/reference.md](ts/doc/reference.md) | [go/doc/reference.md](go/doc/reference.md) |
| Concepts (how & why) | [ts/doc/concepts.md](ts/doc/concepts.md) | [go/doc/concepts.md](go/doc/concepts.md) |

The Go [concepts](go/doc/concepts.md) page includes a "Differences from
the TS version" section (value types, API shape, known differences). The
Rust crate's [README](rs/README.md) carries the same section for that
port.

## Grammar diagram

The installed grammar as a railroad/syntax diagram, generated from the
live grammar with [`@tabnas/railroad`](https://github.com/tabnas/railroad):

![toml grammar railroad diagram](ts/doc/grammar.svg)

ASCII version: [`ts/doc/grammar.txt`](ts/doc/grammar.txt).

## License

MIT. Copyright (c) Richard Rodger and other contributors.
