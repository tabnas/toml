# @jsonic/toml (TypeScript)

This plugin allows the [Jsonic](https://jsonic.senecajs.org) JSON parser
to support [TOML](https://toml.io) syntax.

The documentation follows the [Diataxis](https://diataxis.fr) framework
and is split into four purposes:

- [Tutorial](./ts/tutorial.md) — a step-by-step lesson for newcomers.
- [How-to guides](./ts/how-to.md) — task-oriented recipes for common
  problems.
- [Reference](./ts/reference.md) — the plugin's API, options, and
  value-mapping rules.
- [Explanation](./ts/explanation.md) — discussion of how and why the
  plugin is built the way it is.

## Install

```sh
npm install @jsonic/toml jsonic
```

## At a glance

```js
const { Jsonic } = require('jsonic')
const { Toml } = require('@jsonic/toml')

const toml = Jsonic.make().use(Toml, {})

const result = toml(`
title = "TOML Example"

[owner]
name = "Tom Preston-Werner"
`)
```

For anything beyond this snippet, start with the
[tutorial](./ts/tutorial.md).
