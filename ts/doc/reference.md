# Reference — TypeScript

Complete, dry description of what `@tabnas/toml` exposes, how TOML values
map to JavaScript, and the grammar the plugin accepts. For a tour see the
[tutorial](tutorial.md); for recipes see the [how-to guide](guide.md); for
the rationale see [concepts](concepts.md).

## Package

Published as `@tabnas/toml`. Runtime peer dependencies:

| Peer              | Range  | Role                              |
| ----------------- | ------ | --------------------------------- |
| `@tabnas/parser`  | `>= 2` | The tabnas parsing engine.        |
| `@tabnas/jsonic`  | `>= 2` | The relaxed-JSON base grammar.    |

## Exports

```ts
import { Toml, type TomlOptions } from '@tabnas/toml'
```

| Export        | Kind                      | Description                                              |
| ------------- | ------------------------- | ------------------------------------------------------- |
| `Toml`        | `Plugin` (from `@tabnas/parser`) | Installs the TOML grammar onto a `Tabnas` instance. |
| `TomlOptions` | type                      | Plugin options object. Currently `{}` (no options).     |

`Toml.defaults` is `{}`.

## Installation and parsing

```ts
import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Toml } from '@tabnas/toml'

const toml = new Tabnas().use(jsonic).use(Toml)
const result = toml.parse(src)
```

- `.use(jsonic)` must come before `.use(Toml)`: the plugin sets its `toml`
  rule as the start rule and excludes the `jsonic` start rule.
- `.use(Toml, options?)` accepts an optional `TomlOptions`. It is reserved
  for future use; passing `{}` or omitting it is equivalent.
- `.parse(src: string)` returns the parsed value. For any TOML document
  the root is a plain object. A TOML document never produces a bare
  scalar at the root; every top-level line is a key assignment or a table
  header.
- On a syntax error `.parse` throws; the thrown error has a `code`
  property (e.g. `'unexpected'`) and a formatted multi-line `message`.

## Options

`TomlOptions` is an empty object type — there are no user-tunable options.
The plugin's behaviour is fixed by its grammar.

## Value mapping

| TOML construct                | JavaScript result                              |
| ----------------------------- | ---------------------------------------------- |
| Basic / literal string        | `string`                                       |
| Multi-line string (`"""`/`'''`) | `string`                                     |
| Integer (`42`, `0xff`, `0o17`, `0b101`, `1_000`) | `number`                  |
| Float (`1.5`, `1e10`, `1.5E2`) | `number`                                      |
| `nan`, `+nan`, `-nan`         | `NaN`                                           |
| `inf`, `+inf`                 | `Infinity`                                      |
| `-inf`                        | `-Infinity`                                     |
| Boolean (`true` / `false`)    | `boolean`                                       |
| Array `[ … ]`                 | `Array`                                         |
| Inline table `{ … }`          | plain `object`                                  |
| Table `[a]`                   | plain `object`                                  |
| Nested table `[a.b]`          | plain `object` at `a.b`                         |
| Array of tables `[[a]]`       | `Array` of plain objects                        |
| Dotted key `a.b = 1`          | nested `object`                                 |
| Quoted key `"a b" = 1`        | property named `a b` on the enclosing object    |
| Datetime / time literal       | `Date` with a `__toml__` tag (see below)        |
| Line comment `# …`            | discarded                                       |

Examples (each is a real parse result):

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')
const toml = new Tabnas().use(jsonic).use(Toml)

toml.parse('a = 1.5e2')          // => { a: 150 }
toml.parse('a = "x"')            // => { a: 'x' }
toml.parse('a = true')           // => { a: true }
toml.parse('"key with space" = 1')  // => { 'key with space': 1 }
toml.parse('a = [1, "x", true]') // => { a: [1, 'x', true] }
```

## Datetime metadata

Every datetime or time literal is a JavaScript `Date` decorated with a
`__toml__` property:

```ts
type TomlDateMeta = {
  kind: 'offset-date-time' | 'local-date-time' | 'local-date' | 'local-time'
  src:  string   // original source text
}
```

`kind` is derived from the literal's shape; `src` is the exact source text
so the value can be re-emitted without relying on `Date` formatting.

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')
const toml = new Tabnas().use(jsonic).use(Toml)

const r = toml.parse('dob = 1979-05-27T07:32:00Z')
r.dob.__toml__   // => { kind: 'offset-date-time', src: '1979-05-27T07:32:00Z' }
```

## Accepted syntax

The plugin accepts the core of TOML 1.0:

- **Bare keys**: letters, digits, `_`, `-` (the `#ID` token).
- **Quoted keys**: `"…"` and `'…'`, including spaces and dots inside the
  quotes (`"a.b" = 1` is a single key named `a.b`).
- **Dotted keys**: `a.b.c = 1` builds nested objects.
- **Strings**: basic `"…"`, literal `'…'`, and the triple-quoted
  multi-line forms `"""…"""` / `'''…'''`. A newline immediately after the
  opening triple delimiter is trimmed.
- **Escape sequences** in basic strings: `\b`, `\t`, `\n`, `\f`, `\r`,
  `\"`, `\\`, `\xHH`, `\uXXXX`, `\UXXXXXXXX`, and the multi-line
  "line-ending backslash" that swallows the following whitespace.
- **Integers**: decimal, `0x`, `0o`, `0b`, with optional `_` separators
  and a leading `+`/`-`.
- **Floats**: decimal point and/or exponent (`e`/`E`), plus the keyword
  values `nan` and `inf` with optional sign.
- **Booleans**: `true`, `false`.
- **Arrays**: `[ … ]`, mixed element types, optional trailing comma.
- **Inline tables**: `{ key = value, … }`.
- **Tables**: `[name]`, dotted `[a.b.c]`.
- **Array of tables**: `[[name]]`, repeated to append.
- **Comments**: `#` to end of line; the slash (`//`) and multi-line
  comment forms inherited from jsonic are disabled.
- **Datetimes / times**: RFC-3339-style date, date-time, and time
  literals.

Date-shaped *keys* are handled too: `2001-02-03 = 1` and `[2002-01-02]`
treat the date-shaped text as a bare key rather than a value, because the
date matchers defer to bare-key lexing whenever the current grammar
position expects a key.

### Token legend

The plugin describes its key token for the railroad diagram legend:

| Token | Meaning                                |
| ----- | -------------------------------------- |
| `#ID` | bare key: letters, digits, `_` or `-`  |

## Errors

A malformed document throws. The error code for an unexpected token or
character is `unexpected`:

```js
const { Tabnas } = require('@tabnas/parser')
const { jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')
const toml = new Tabnas().use(jsonic).use(Toml)

let code
try { toml.parse('= 1') } catch (e) { code = e.code }
code   // => 'unexpected'
```

## Grammar source and embedding

The canonical grammar is `toml-grammar.jsonic` at the repository root,
written in jsonic syntax. `ts/embed-grammar.js` copies it verbatim into
`ts/src/toml.ts` between these markers:

```
// --- BEGIN EMBEDDED toml-grammar.jsonic ---
// --- END EMBEDDED toml-grammar.jsonic ---
```

Run `npm run embed` (or `npm run build`, which embeds then compiles) after
editing the grammar. The same file is embedded into the Go port, so the
two runtimes stay in sync.

## npm scripts

| Script                      | Purpose                                   |
| --------------------------- | ----------------------------------------- |
| `npm run build`             | Embed grammar, compile `src` + `test`.    |
| `npm test`                  | Run the compiled test suite.              |
| `npm run watch`             | Incrementally compile on change.          |
| `npm run embed`             | Re-embed the grammar only.                |
| `npm run install-toml-test` | Clone BurntSushi's `toml-test` suite.     |
| `npm run clean`             | Remove build artefacts and `node_modules`.|
| `npm run reset`             | clean → install → fetch suite → build → test. |
