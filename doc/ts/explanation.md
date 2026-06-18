# Explanation — TypeScript design

Background and rationale for readers who want to understand why
`@tabnas/toml` is built the way it is. For task-focused material, go to
the [how-to guides](./how-to.md) and [reference](./reference.md).

## Why a Jsonic plugin?

TOML is a configuration syntax with overlapping structure to JSON:
objects (tables), arrays, scalars. Rather than write a standalone
parser, this plugin reuses [Tabnas](https://github.com/tabnas/jsonic)'s
declarative grammar engine and adds the handful of rules that make TOML
TOML: sectioned tables, dotted keys, array-of-tables, datetime literals,
and a few string escapes.

Reusing Jsonic gives us:

- A well-tested lexer and rule engine.
- A grammar that is declarative (it is itself a Jsonic document), not
  executable JavaScript.
- Consistency with the other `@tabnas/*` plugins.

## Single source of truth for the grammar

`toml-grammar.jsonic` at the repository root is the canonical grammar.
The `embed-grammar.js` script copies it verbatim into two ports:

- `src/toml.ts` (this TypeScript plugin).
- `go/toml.go` (the Go port).

Both ports therefore share one grammar. If you only ever edit the
generated `const grammarText = ...` block in a source file, your change
will be overwritten the next time `embed-grammar.js` runs. Always edit
`toml-grammar.jsonic`.

## How the plugin hooks Jsonic

At load time the plugin:

1. Parses the embedded grammar using a jsonic-grammar engine
   (`new Tabnas().use(jsonic).parse(grammarText)`).
2. Attaches a `refs` map of named function references that the grammar
   text points at (e.g. `@isodate-val`, `@make-toml-string-matcher`).
3. Patches in `NaN` / `Infinity` value definitions that cannot be
   expressed literally in a Jsonic document (they don't survive a
   round-trip through Jsonic parsing).
4. Calls `tn.grammar(grammarDef)` to install everything on the
   caller's Tabnas instance.

The grammar definition itself describes rules such as `toml`, `table`,
`pair`, `dive`, `map`, and `val`. Each rule has `open` and/or `close`
alternatives listing token sequences and actions.

## Datetimes as tagged `Date`s

TOML has four datetime variants (offset-datetime, local-datetime,
local-date, local-time). JavaScript has one `Date`. The plugin returns
a `Date` with a non-enumerable-ish sidecar `__toml__` property:

```ts
{
  kind: 'offset-date-time' | 'local-date-time' | 'local-date' | 'local-time',
  src:  string
}
```

The alternative — a custom class — was rejected because it breaks the
"TOML parses to plain JSON-like values" expectation and confuses
downstream code that does `instanceof Date` or `value.toISOString()`.
Keeping the `Date` and attaching metadata lets both audiences win:
naïve callers see a standard `Date`; tooling that needs to preserve the
TOML distinction can look at `__toml__`.

## The custom string matcher

Jsonic's built-in string lexer handles JSON-style strings. TOML adds
literal strings (`'...'`), triple-quoted multi-line strings, a
line-ending backslash for continuations, and an `\xHH` byte escape. The
plugin installs a custom `stringMatcher` (adapted from
[huan231/toml-nodejs](https://github.com/huan231/toml-nodejs)) that
understands all of these. The matcher is wired in through
`options.lex.match.string.make`.

## Scope and non-goals

- The plugin targets the TOML 1.0 feature set.
- It does not attempt to produce TOML — it only parses.
- There are no configurable options today. `TomlOptions` is an empty
  object type so future extension is source-compatible.

## Relationship to the Go port

The Go port (`go/toml.go`) is a sibling, not a dependency. It embeds
the same grammar text, resolves the same `@<ref>` names to Go
functions, and returns Go `map[string]any`. Behavioural differences are
bugs — both ports run against a shared feature matrix and the BurntSushi
`toml-test` suite.
