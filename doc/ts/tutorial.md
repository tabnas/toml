# Tutorial — Parsing TOML with @tabnas/toml

This lesson walks through the minimum you need to parse a real TOML
document. By the end you will have a Node.js script that turns TOML
source into a JavaScript object and pretty-prints it.

No prior Jsonic knowledge is required. You should have Node.js 20 or
later installed.

## 1. Create a project

```sh
mkdir toml-demo
cd toml-demo
npm init -y
npm install @tabnas/toml @tabnas/jsonic
```

## 2. Write your first parser

Create `demo.js`:

```js
const { Jsonic } = require('@tabnas/jsonic')
const { Toml } = require('@tabnas/toml')

const toml = Jsonic.make().use(Toml, {})

const src = `
title = "TOML Example"

[owner]
name = "Tom Preston-Werner"
dob  = 1979-05-27T07:32:00Z
`

const result = toml(src)
console.log(JSON.stringify(result, null, 2))
```

Run it:

```sh
node demo.js
```

You should see:

```json
{
  "title": "TOML Example",
  "owner": {
    "name": "Tom Preston-Werner",
    "dob": "1979-05-27T07:32:00.000Z"
  }
}
```

(`dob` is a standard JavaScript `Date`; its `toJSON` is what
`JSON.stringify` prints.)

## 3. Add structure: tables and arrays

Replace the contents of `src` with:

```toml
[database]
server = "192.168.1.1"
ports  = [8001, 8001, 8002]
enabled = true

[[products]]
name = "Hammer"

[[products]]
name = "Nail"
```

Re-running `node demo.js` now gives:

```json
{
  "database": {
    "server": "192.168.1.1",
    "ports": [8001, 8001, 8002],
    "enabled": true
  },
  "products": [
    { "name": "Hammer" },
    { "name": "Nail" }
  ]
}
```

You have just parsed key-value pairs, nested tables, arrays, booleans,
and an array of tables.

## 4. Inspect a TOML date

TOML distinguishes offset, local, and date-only values. The plugin
returns a `Date` object with a `__toml__` tag attached so you can tell
them apart:

```js
const { dob } = result.owner
console.log(dob.__toml__)
// { kind: 'offset-date-time', src: '1979-05-27T07:32:00Z' }
```

The `kind` field is one of `offset-date-time`, `local-date-time`,
`local-date`, or `local-time`.

## Where to go next

- [How-to guides](./how-to.md) for recipes such as parsing a file from
  disk or handling errors.
- [Reference](./reference.md) for the full API and value-mapping rules.
- [Explanation](./explanation.md) for how the plugin is structured and
  why it uses Jsonic's declarative grammar.
