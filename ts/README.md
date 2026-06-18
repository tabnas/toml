# @tabnas/toml (TypeScript / JavaScript)

A [TOML](https://toml.io) parser, built as a
[Jsonic](https://github.com/tabnas/jsonic) grammar plugin on the
[tabnas](https://github.com/tabnas/parser) engine. Parses TOML source
into plain JavaScript objects.

[![npm version](https://img.shields.io/npm/v/@tabnas/toml.svg)](https://npmjs.com/package/@tabnas/toml)
[![build](https://github.com/tabnas/toml/actions/workflows/build.yml/badge.svg)](https://github.com/tabnas/toml/actions/workflows/build.yml)

## Install

```sh
npm install @tabnas/toml @tabnas/parser @tabnas/jsonic
```

## Example

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = 1\nb = [2, 3]')   // => { a: 1, b: [2, 3] }
```

Build the `toml` instance once and reuse it; building it installs the
whole grammar, while parsing is cheap.

## Documentation

Organised around the [Diataxis](https://diataxis.fr) framework:

- [Tutorial](doc/tutorial.md) — a step-by-step first parse.
- [How-to guide](doc/guide.md) — task-oriented recipes.
- [Reference](doc/reference.md) — the API, value mapping, and accepted
  grammar.
- [Concepts](doc/concepts.md) — how the plugin works and why.

## Options

_None._ `TomlOptions` is currently an empty object type.

## Grammar diagram

The installed grammar as a railroad/syntax diagram, generated with
[`@tabnas/railroad`](https://github.com/tabnas/railroad):

![toml grammar railroad diagram](doc/grammar.svg)

A vertical ASCII version is in [`doc/grammar.txt`](doc/grammar.txt).

## References

String handling adapted from <https://github.com/huan231/toml-nodejs>
(MIT).

## License

MIT. Copyright (c) Richard Rodger and other contributors.
