# @tabnas/toml

This plugin allows the [Jsonic](https://jsonic.senecajs.org) JSON parser to support toml syntax.

This repository contains:

| Path | Description |
|---|---|
| [`ts/`](ts/) | TypeScript / JavaScript implementation. |
| [`go/`](go/) | Go port. |
| [`test/spec/`](test/spec/) | Shared conformance fixtures, exercised by both runtimes. |

See [`ts/README.md`](ts/README.md) for usage.

## Grammar

The grammar is defined once in the top-level
[`toml-grammar.jsonic`](toml-grammar.jsonic) and embedded into both the
TypeScript ([`ts/src/toml.ts`](ts/src/toml.ts)) and Go
([`go/toml.go`](go/toml.go)) implementations by
[`ts/embed-grammar.js`](ts/embed-grammar.js), so the two runtimes stay in
sync.

## Grammar diagram

The grammar as a railroad/syntax diagram, generated from the live grammar
with [`@tabnas/railroad`](https://github.com/tabnas/railroad):

![toml grammar railroad diagram](ts/doc/grammar.svg)

ASCII version: [`ts/doc/grammar.txt`](ts/doc/grammar.txt).

## License

MIT. Copyright (c) Richard Rodger.
