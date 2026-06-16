# Reference — TypeScript API

Technical description of what `@tabnas/toml` exposes and how TOML values
are mapped to JavaScript. For tours and recipes, see the
[tutorial](./tutorial.md) and [how-to guides](./how-to.md).

## Exports

```ts
import { Toml, type TomlOptions } from '@tabnas/toml'
```

| Export        | Kind     | Description                                           |
| ------------- | -------- | ----------------------------------------------------- |
| `Toml`        | `Plugin` | Jsonic plugin that installs the TOML grammar.         |
| `TomlOptions` | type     | Options object accepted by the plugin (currently `{}`). |

## Installation signature

```ts
new Tabnas().use(jsonic).use(Toml, options?: TomlOptions)
```

Returns the Tabnas instance with the TOML grammar applied. Call
`.parse(src)` on that instance to parse TOML source strings.

## Options

`TomlOptions` is currently an empty object type; there are no
user-tunable options. The plugin's `Toml.defaults` is `{}`.

## Value mapping

| TOML construct          | JavaScript result                               |
| ----------------------- | ----------------------------------------------- |
| String (basic/literal)  | `string`                                        |
| Multi-line string       | `string`                                        |
| Integer                 | `number`                                        |
| Float                   | `number`                                        |
| `nan`, `+nan`, `-nan`   | `NaN`                                           |
| `inf`, `+inf`           | `Infinity`                                      |
| `-inf`                  | `-Infinity`                                     |
| Boolean                 | `boolean`                                       |
| Array                   | `Array`                                         |
| Inline table            | plain `object`                                  |
| Table `[a]`             | plain `object`                                  |
| Nested table `[a.b]`    | plain `object` at `a.b`                         |
| Array of tables `[[a]]` | `Array` of plain objects                        |
| Dotted key              | nested `object`                                 |
| Quoted key              | property on the enclosing object                |
| Datetime / time literal | `Date` with `__toml__` metadata (see below)     |
| Line comment `#`        | discarded                                       |

## Datetime metadata

Every datetime or time literal is returned as a `Date` decorated with a
`__toml__` property:

```ts
type TomlDateMeta = {
  kind: 'offset-date-time' | 'local-date-time' | 'local-date' | 'local-time'
  src:  string   // original source text
}
```

The original source is preserved so that the (lossy) `Date` object can
be round-tripped back into TOML if needed.

## Integer forms

Decimal, hexadecimal (`0x`), octal (`0o`), binary (`0b`), and
underscore separators (`1_000`) are all accepted and parsed to the same
`number` type.

## Escape sequences in basic strings

`\b`, `\t`, `\n`, `\f`, `\r`, `\"`, `\\`, `\xHH`, `\uXXXX`,
`\UXXXXXXXX`, and the multi-line "line-ending backslash" form are all
honoured.

## Grammar embedding

The canonical grammar lives in `toml-grammar.jsonic` at the repository
root. The script `embed-grammar.js` copies it verbatim into
`src/toml.ts` between marker comments:

```
// --- BEGIN EMBEDDED toml-grammar.jsonic ---
// --- END EMBEDDED toml-grammar.jsonic ---
```

Run `npm run embed` (or `make build-ts`, which does it as part of the
build) after editing the grammar.

## npm scripts

| Script                         | Purpose                                     |
| ------------------------------ | ------------------------------------------- |
| `npm run build`                | Embed grammar and compile `src` + `test`.   |
| `npm test`                     | Run the compiled test suite.                |
| `npm run watch`                | Incrementally compile on change.            |
| `npm run install-toml-test`    | Clone BurntSushi's `toml-test` suite.       |
| `npm run clean`                | Remove build artefacts and `node_modules`.  |
| `npm run reset`                | `clean` → install → fetch suite → build → test. |

## Makefile targets

See [`Makefile`](../../Makefile). Relevant TypeScript targets:

| Target            | Purpose                              |
| ----------------- | ------------------------------------ |
| `make build-ts`   | Equivalent to `npm run build`.       |
| `make test-ts`    | Equivalent to `npm test`.            |
| `make clean-ts`   | Remove `dist/` and `dist-test/`.     |
| `make reset`      | Full rebuild of both TS and Go.      |

## Peer dependencies

`@tabnas/parser >= 2` (the engine) and `@tabnas/jsonic >= 2` (the
grammar provider) are required at runtime.
