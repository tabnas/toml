# Concepts — TypeScript

Background reading: how the plugin works on the engine, the grammar model
it uses, and why certain inputs are accepted or rejected. For steps see
the [tutorial](tutorial.md); for recipes see the [how-to guide](guide.md);
for exact signatures see the [reference](reference.md).

## A grammar plugin, not a parser

`@tabnas/toml` does not contain a parser. It is a **grammar plugin** for
the tabnas engine (`@tabnas/parser`), layered on the relaxed-JSON grammar
that `@tabnas/jsonic` provides. The engine supplies the lexer, the
rule-driven parser, options handling, and error formatting. The plugin
supplies the TOML grammar and a handful of custom lexer matchers.

That is why the install chain is `new Tabnas().use(jsonic).use(Toml)`:
`jsonic` registers the base grammar, then `Toml` overrides the start rule
(`options.rule.start = toml`) and excludes jsonic's own start rule. From
that point on, the same engine parses TOML.

## The grammar is data

The TOML grammar lives in one file, `toml-grammar.jsonic`, written in
jsonic syntax — it *is* a relaxed-JSON document describing rules, tokens,
and options. At build time `embed-grammar.js` inlines that text into the
plugin source. At load time the plugin does something neat: it parses the
grammar text *with jsonic itself*

```ts
const grammarDef = new Tabnas().use(jsonic).parse(grammarText)
grammarDef.ref = refs
tn.grammar(grammarDef)
```

so the grammar definition is bootstrapped by the same machinery it
configures. Function hooks referenced in the grammar by `@name` strings
(`@table-dive-start`, `@lte-table-dive`, …) are resolved against the
`refs` map. A second, identical embed lives in the Go port — keeping the
grammar as shared data is what keeps the two runtimes in sync.

## Two stages: lexer then parser

A parse runs the engine's two cooperating stages.

The **lexer** turns source text into tokens via independent matchers,
each trying to claim text at the current position in priority order. TOML
adds or replaces several matchers:

- a **custom string matcher** (`makeTomlStringMatcher`) that handles
  single- and double-quoted strings *and* the triple-quoted multi-line
  forms, escape sequences, and the line-ending backslash — none of which
  the default jsonic string lexer knows about;
- an `#ID` **bare-key token** matched by `/^[a-zA-Z0-9_-]+/`;
- **date / time value matchers** for the RFC-3339 shapes;
- fixed tokens `=` (`#CL`) and `.` (`#DOT`), which TOML needs as
  structural punctuation rather than plain text.

The **parser** then consumes tokens according to named rules. Each rule
has an **open** and a **close** phase, each a list of **alternates**; an
alternate matches on a short token lookahead (`s`), may carry a condition
(`c`), and on success can push a child rule (`p`), replace itself with
another (`r`), back up tokens (`b`), set counters (`n`), or run an action
(`a`). The TOML rules are `toml` (the start), `table`, `map`, `pair`,
`val`, `elem`, and `dive`.

## The grammar model: tables and dives

TOML's shape is unusual because *position in the document* determines
*position in the tree*. Two mechanisms in the grammar handle this:

- **Tables.** A `[a.b]` header walks (or creates) the path `a → b` and
  makes the following pairs land inside it; `[[a]]` does the same but
  appends a fresh object to an array at `a`. The `table` rule does this
  with counter-guarded alternates (`table_dive`, `table_array`) and the
  `@table-dive-*` / `@table-key-cs-*` actions that build or reuse the
  nested objects and arrays.
- **Dotted keys.** `a.b.c = 1` is handled by the `dive` rule, which
  recurses on each `.` segment (bounded by the `dive_key` counter),
  creating an object per segment and assigning the value at the leaf.

Both reduce to the same idea: consume key segments, descend or allocate
nested objects as you go, and place the value at the bottom. The result
is always a plain nested object tree.

## Why date literals need special handling

A value matcher fires unconditionally — so a date-shaped token like
`1979-05-27` would be claimed as a datetime value before the `#ID` bare-key
matcher ever runs. That is wrong when the date shape is actually a *key*,
as in `2001-02-03 = 1` or the table header `[2002-01-02]`.

The plugin solves this with context awareness. After installing the
grammar it swaps the regex date/time matchers for function matchers that
first ask `isKeyContext`: does the rule at the current position expect an
`#ID` token? Value-producing rules (`val`, `list`, `elem`) never list
`#ID`; key-accepting rules (`toml`, `map`, `dive`, `pair`, `table`) do. If
a key is expected, the date matcher returns `null` and lets the bare-key
matcher take over; otherwise it emits the datetime value. This is why
date-shaped keys parse correctly.

## Why NaN and Infinity are patched in code

The grammar text is itself parsed by jsonic, and `NaN` / `Infinity`
cannot round-trip through a JSON-ish parse (they would come back as the
strings `"NaN"` / `"Infinity"` or as `null`). So the `nan` / `inf` value
keywords are installed as real JavaScript values *after* the grammar is
loaded, by patching `grammarDef.options.value`. The signed forms (`+nan`,
`-inf`, …) are wired the same way.

## Accepted vs rejected

Accepted (all real, all covered by the test suites):

- bare, quoted, and dotted keys — including a quoted key that contains a
  dot, `"a.b" = 1`, which is one key, not a path;
- integers in decimal, hex, octal, binary, with `_` separators;
- floats with exponents, and `nan` / `inf` with signs;
- basic, literal, and triple-quoted strings with the full escape set;
- arrays (mixed types, trailing comma), inline tables, tables, nested
  tables, and array-of-tables;
- `#` line comments, anywhere a comment is legal;
- offset / local date-time / local-date / local-time literals — and the
  same shapes used as keys.

Rejected (throws with code `unexpected`):

- a key with no value: `a =`;
- a value with no key: `= 1`;
- an unterminated string: `"unterminated`.

The slash (`//`) and block comment forms that jsonic enables by default
are switched off, because TOML comments use `#` only.

## Design rationale

- **One grammar, two runtimes.** The grammar is authored once as data and
  embedded into both the TS and Go ports, so behaviour matches by
  construction and is checked against shared `test/spec/*.tsv` fixtures
  plus the external BurntSushi `toml-test` conformance suite.
- **Reuse the engine.** Building on tabnas + jsonic means TOML gets the
  engine's lexer, error reporting, and rule machinery for free, and the
  plugin only has to express what is *different* about TOML.
- **Preserve source for dates.** `Date` is lossy (it normalises offsets
  and drops the local/offset distinction), so the original text is kept in
  `__toml__.src` for faithful round-tripping.
