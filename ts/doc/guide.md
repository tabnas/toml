# How-to guide — TypeScript

Task-oriented recipes. Each is self-contained. For a guided introduction
see the [tutorial](tutorial.md); for complete signatures and the grammar
see the [reference](reference.md).

Every recipe assumes these three packages are installed:

```sh
npm install @tabnas/toml @tabnas/parser @tabnas/jsonic
```

## Build a parser

`Toml` is a Jsonic plugin, not a standalone function. Compose it onto a
`Tabnas` engine instance after `jsonic`, then call `.parse()`:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = 1\nb = 2')   // => { a: 1, b: 2 }
```

In TypeScript the imports are identical; use `import` instead of
`require`.

## Reuse one parser for many parses

Building the parser installs the whole TOML grammar, which is the costly
step. A parse only reads instance state, so build once and reuse:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

const parseConfig = (src) => toml.parse(src)

parseConfig('port = 8080')   // => { port: 8080 }
parseConfig('host = "db"')   // => { host: 'db' }
```

## Parse a file from disk

`.parse()` takes a string, so read the file first:

```js ignore
const { readFileSync } = require('node:fs')
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)
const config = toml.parse(readFileSync('config.toml', 'utf8'))
```

## Handle a parse error

A syntax error throws. The thrown error carries a `code` you can branch
on; malformed input gives `unexpected`:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

let code
try {
  toml.parse('a = ')   // missing value
} catch (err) {
  code = err.code
}
code   // => 'unexpected'
```

The error's `message` is a formatted, multi-line diagnostic pointing at
the offending line and column — print it as-is for a readable report.

## Read integers in every base

Decimal, hex (`0x`), octal (`0o`), and binary (`0b`) all parse to the same
JavaScript `number`; `_` digit separators are allowed:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = 0xff')      // => { a: 255 }
toml.parse('a = 0o17')      // => { a: 15 }
toml.parse('a = 0b101')     // => { a: 5 }
toml.parse('a = 1_000')     // => { a: 1000 }
```

`nan`, `inf`, and their signed forms (`+nan`, `-inf`, …) parse to the
JavaScript `NaN`, `Infinity`, and `-Infinity` values.

## Read a date or time and tell its kind

Date and time literals come back as a JavaScript `Date` carrying a
`__toml__` tag. Use `kind` to distinguish the four TOML shapes:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = 1979-05-27').a.__toml__.kind            // => 'local-date'
toml.parse('a = 1979-05-27T07:32:00').a.__toml__.kind   // => 'local-date-time'
toml.parse('a = 1979-05-27T07:32:00Z').a.__toml__.kind  // => 'offset-date-time'
toml.parse('a = 07:32:00').a.__toml__.kind              // => 'local-time'
```

`__toml__.src` holds the original text, so you can re-emit the value
exactly or feed it to a dedicated date library.

## Use multi-line and literal strings

Basic strings (`"…"`) honour escapes; literal strings (`'…'`) are taken
verbatim; the triple-quoted forms span lines:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = "tab\\there"')   // => { a: 'tab\there' }
toml.parse("a = 'literal'")       // => { a: 'literal' }
toml.parse('a = """hello"""')     // => { a: 'hello' }
```

## Compare to the canonical instance form

The `new Tabnas().use(jsonic).use(Toml)` chain is the supported public
form. The same instance handles tables, dotted keys, and inline tables
without any extra configuration:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a.b.c = 2')           // => { a: { b: { c: 2 } } }
toml.parse('a = {x = 1, y = 2}')  // => { a: { x: 1, y: 2 } }
```
