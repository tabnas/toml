# Tutorial — your first TOML parse

This walks you from nothing to a working parse. Follow it in order; each
step builds on the last. When you finish you will have installed the
plugin, parsed a TOML document into a JavaScript object, and looked at how
a TOML date comes back.

No prior Jsonic knowledge is required. You need Node.js 24 or later.

For a recipe-style index of individual tasks, see the
[how-to guide](guide.md). For exhaustive signatures and the full grammar,
see the [reference](reference.md).

## 1. Install

`@tabnas/toml` is a grammar plugin. It needs the tabnas engine
(`@tabnas/parser`) and the relaxed-JSON grammar it layers on
(`@tabnas/jsonic`):

```sh
npm install @tabnas/toml @tabnas/parser @tabnas/jsonic
```

## 2. Parse a string

Build a parser by chaining `.use(jsonic)` then `.use(Toml)` onto a fresh
`Tabnas` instance, then call `.parse(src)`:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = 1')   // => { a: 1 }
```

You wrote one key-value pair and got back an object. The order matters:
`jsonic` supplies the base grammar, and `Toml` replaces its start rule
with the TOML one. Reuse the `toml` instance for every parse — building it
is the expensive part.

## 3. Parse a real document

TOML documents combine pairs, tables, and arrays. The plugin maps a whole
document to a nested object:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

const src = `
title = "TOML Example"

[owner]
name = "Tom"
`

toml.parse(src)   // => { title: 'TOML Example', owner: { name: 'Tom' } }
```

A bare `key = value` line at the top sets a property on the root object. A
`[table]` header opens a nested object that following pairs go into.

## 4. Add arrays and an array of tables

Arrays use `[ ... ]`; a repeated `[[name]]` header builds an array of
tables:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = [1, 2, 3]')   // => { a: [1, 2, 3] }

const src = `
[[products]]
name = "Hammer"

[[products]]
name = "Nail"
`

toml.parse(src)   // => { products: [ { name: 'Hammer' }, { name: 'Nail' } ] }
```

You have now parsed key-value pairs, a nested table, an array, and an
array of tables.

## 5. Look at a TOML date

TOML has first-class date and time literals. The plugin returns a standard
JavaScript `Date`, with a `__toml__` tag attached so you can tell an
offset datetime from a local date:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

const result = toml.parse('dob = 1979-05-27T07:32:00Z')

result.dob.__toml__.kind   // => 'offset-date-time'
result.dob.__toml__.src    // => '1979-05-27T07:32:00Z'
```

`kind` is one of `offset-date-time`, `local-date-time`, `local-date`, or
`local-time`. `src` is the original source text, kept so the (lossy) date
can be round-tripped.

## Where to go next

- [How-to guide](guide.md) — recipes: parse a file, handle errors, share a
  parser, read dates.
- [Reference](reference.md) — the full API, the value-mapping table, and
  the accepted grammar.
- [Concepts](concepts.md) — how the plugin works on the engine and why it
  is built the way it is.
