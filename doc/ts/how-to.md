# How-to guides — TypeScript

Short, task-oriented recipes. Each guide assumes you already have
`@tabnas/toml` installed and a Jsonic instance created:

```js
const { Jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')
const toml = Jsonic.make().use(Toml, {})
```

## Parse a TOML file from disk

```js
const fs = require('node:fs')
const result = toml(fs.readFileSync('config.toml', 'utf8'))
```

TOML does not ship with a streaming model — load the whole file, then
parse.

## Reuse a parser across many calls

`Jsonic.make().use(Toml, {})` does non-trivial grammar setup. Create
it once and reuse:

```js
const toml = Jsonic.make().use(Toml, {})

function loadConfig(src) {
  return toml(src)
}
```

## Use ES module syntax

```js
import { Jsonic } from '@tabnas/jsonic'
import { Toml } from '@tabnas/toml'

const toml = Jsonic.make().use(Toml, {})
```

The package is published as CommonJS with a `.d.ts` typings file, so
both `require` and `import` work from TypeScript and Node 20+.

## Handle parse errors

Jsonic throws on invalid input. Wrap calls in `try`/`catch`:

```js
try {
  const result = toml(src)
} catch (err) {
  console.error('TOML parse failed:', err.message)
}
```

The thrown error carries the standard Jsonic shape (`code`, `details`,
source location).

## Distinguish TOML dates from plain JavaScript Dates

Every value produced from a TOML datetime or time literal is a `Date`
with a `__toml__` tag:

```js
const result = toml('a = 1979-05-27T07:32:00Z')
const kind = result.a.__toml__.kind    // 'offset-date-time'
const src  = result.a.__toml__.src     // '1979-05-27T07:32:00Z'
```

Use this when round-tripping values back into TOML, or when preserving
whether the original was local or offset.

## Round-trip special floats

TOML allows `nan`, `+nan`, `-nan`, `inf`, `+inf`, `-inf`. The plugin
parses all six into their JavaScript equivalents (`NaN`, `Infinity`,
`-Infinity`). No extra configuration is needed.

## Run the BurntSushi TOML test suite locally

The test suite is external; pull it and run it with:

```sh
npm run install-toml-test
npm run build
npm test
```

Or use the Makefile:

```sh
make test-ts
```

## Contribute a grammar fix

1. Edit `toml-grammar.jsonic` at the repository root.
2. Run `node embed-grammar.js` (or `npm run embed`) to sync the
   embedded copies in `src/toml.ts` and `go/toml.go`.
3. Run `make test` to exercise both the TypeScript and Go ports.
