# tabnas-toml (Rust)

The [TOML](https://toml.io) grammar plugin for the
[`tabnas`](https://github.com/tabnas/parser) parsing engine, crate
`tabnas_toml`.

It reads TOML into plain values: bare, quoted and dotted keys, `[table]`
headers, `[[array-of-tables]]`, inline tables, arrays, basic, literal and
multi-line strings, integers and floats including `inf` and `nan`,
booleans, and date and time values.

It is not standalone. The relaxed-JSON core (`val` / `map` / `list` /
`pair` / `elem`) comes from the
[`tabnas-jsonic`](https://github.com/tabnas/jsonic) plugin; this crate
installs the TOML rules over it, sets the start rule to `toml`, and adds
a TOML string matcher and context-aware date and time matchers.

This is the Rust port of the canonical TypeScript implementation in
[`../ts`](../ts); the TypeScript version is authoritative and this crate
tracks it. The Go port is in [`../go`](../go). All three embed the same
grammar, authored once in
[`../toml-grammar.jsonic`](../toml-grammar.jsonic).

## Use

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let value = tabnas_toml::parse("title = \"TOML\"\n[owner]\nname = \"Tom\"")?;
    assert_eq!(
        value.to_string(),
        r#"{"title":"TOML","owner":{"name":"Tom"}}"#
    );
    Ok(())
}
```

`parse` reuses one shared instance, because building the grammar costs
much more than a parse. To hold your own instance, build one:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = tabnas_toml::make();
    assert_eq!(
        parser.parse("[[products]]\nname = \"Hammer\"")?.to_string(),
        r#"{"products":[{"name":"Hammer"}]}"#
    );
    assert_eq!(
        parser.parse("a.b.c = 1")?.to_string(),
        r#"{"a":{"b":{"c":1}}}"#
    );
    Ok(())
}
```

To layer TOML on an instance of your own, install the plugin, or call the
grammar function directly:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = tabnas_jsonic::make();
    parser.use_plugin(tabnas_toml::plugin(), None)?;
    assert_eq!(parser.parse("a = 1")?.to_string(), r#"{"a":1}"#);

    let mut direct = tabnas_jsonic::make();
    tabnas_toml::toml(&mut direct)?;
    assert_eq!(direct.parse("[t]\nx = 1")?.to_string(), r#"{"t":{"x":1}}"#);
    Ok(())
}
```

A date, time or date-time carries its kind alongside its source text.
Read it with `toml_time`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let value = tabnas_toml::parse("when = 1979-05-27T07:32:00Z")?;
    let tabnas::Value::Object(entries) = &value else {
        panic!("a TOML document is a table");
    };
    let when = tabnas_toml::toml_time(&entries["when"]).expect("a date-time");
    assert_eq!(when.kind, tabnas_toml::OFFSET_DATE_TIME);
    assert_eq!(when.src, "1979-05-27T07:32:00Z");
    Ok(())
}
```

Parse errors are the engine's `TabnasError`, re-exported as `TomlError`,
with `code`, `row`, `col` and a report that shows the offending source
with a caret under the `[toml/<code>]` tag. Beyond the engine's own
codes, this plugin raises `toml_key_conflict` for a key TOML does not
allow to be redefined, `invalid_datetime` for a date or time whose
components cannot denote a real instant, and `unterminated_string`,
`unprintable`, `invalid_ascii` and `invalid_unicode` from the string
matcher.

## Install

None of these crates is published to a registry, so they are consumed as
**sibling checkouts**, the standard tabnas development model. Clone all
three of `https://github.com/tabnas/parser`,
`https://github.com/tabnas/jsonic` and `https://github.com/tabnas/json`
next to this repository, then point at the two you name directly:

```toml
[dependencies]
tabnas-toml = { path = "../toml/rs" }
tabnas-jsonic = { path = "../jsonic/rs" }
tabnas = { path = "../parser/rs" }
```

All three entries are needed for the examples above. A crate's
dependencies are not passed on to its dependents, so `tabnas-toml` alone
does not put `tabnas` or `tabnas_jsonic` in your extern prelude. Only
`TomlError` is re-exported.

`json` is the one checkout with no entry in that table, and it is not
optional: `tabnas-jsonic` reaches the strict-JSON core through its own
path dependency on `../../json/rs`. Cargo reads every manifest in the
graph before it compiles anything, so without that checkout the build
stops at `failed to get tabnas-json as a dependency of package
tabnas-jsonic` while resolving, and no example is ever reached.

Running this crate's own test suite needs a fourth checkout,
`https://github.com/tabnas/support`, which holds the shared fixture
loader and runner. It is a development dependency, so a crate that only
consumes the library does not need it.

## Differences from the canonical TypeScript

The shared fixtures in [`../test/spec`](../test/spec) hold all three
runtimes to one answer, and the
[BurntSushi/toml-test](https://github.com/BurntSushi/toml-test) corpus
holds this crate to the same counts as TypeScript: every one of its 268
valid documents, and the same 278 rejections of its 509 invalid ones,
recorded in [`../test/conformance.tsv`](../test/conformance.tsv). What
differs is the shape of the API and a few points where the host language
has no way to say what JavaScript says:

- **A date is a `Text` value, not a `Date`.** `tabnas::Value` is a closed
  enum with no room for a host type, so a date, time or date-time travels
  as a `Value::Text` whose `string` is the source and whose `quote` is
  the kind. `toml_time` reads it back. It renders as the source text,
  which is close to what `JSON.stringify` does to the TypeScript `Date`
  and more readable than the Go struct.
- **Configuration is a type, not an options object.** `TomlOptions`
  carries nothing in any port; `make_with` takes it so that adding an
  option later is not a breaking change.
- **Key order is document order**, because the result is built on an
  `IndexMap`.
- **A table node is a path, not a reference.** A `Value` container is
  behind an `Arc` and copies when a second handle writes to it, so a
  table rule holds the route from the document root to its table rather
  than a handle on it. The document that comes out is the same one.
- **Columns count Unicode scalar values.** An astral character advances
  the column by one, where TypeScript counts UTF-16 units and advances by
  two. That is the engine's unit, recorded in its own
  `DIVERGENCE.md` and in [`../test/divergent.tsv`](../test/divergent.tsv),
  which every runtime executes.
- **Malformed UTF-8 cannot reach the parser.** The engine parses a
  `&str`, so a document that is not valid UTF-8 has to be decoded before
  it arrives. The conformance suite decodes with replacement characters,
  which is what Node hands the TypeScript suite.
- **Lone surrogates fold to U+FFFD**, and the regular expression dialect
  is the `regex` crate's. Both come from the engine, and both are
  recorded there. A lone surrogate is the one difference the register
  cannot carry: its cells are JSON, and the JSON decoders that read them
  fold a lone surrogate to U+FFFD as well, so every cell would compare
  equal. Go answers U+FFFD here too.
- **A lax hexadecimal escape follows TypeScript.** `"\u12g4"` reads as
  U+0012 in TypeScript and here, because the canonical scan accepts every
  ASCII letter and then takes the longest hexadecimal prefix. Go rejects
  it. The row in [`../test/divergent.tsv`](../test/divergent.tsv) records
  the three answers, and **Go's rejection is the repair target**: TOML
  requires exactly four hexadecimal digits, so `"\u12g4"` is not a TOML
  document and Go is the only one of the three that says so. The ports
  that move are TypeScript and this one.
- **A bad token reached in lookahead reports `unexpected`.** When an
  alternate needs two tokens and the second is a bad token from the
  lexer, the other two ports raise that token's own code, such as
  `unterminated_string` for `["abc`, and this port names the first token
  instead. The engine builds its "no alternate matched" error from that
  first token, so the diagnosis of a later one is lost. Six documents of
  the conformance corpus land here, every one of them still rejected, and
  the three rows in [`../test/divergent.tsv`](../test/divergent.tsv) pin
  the difference until the engine repair lands.

## Build and test

The engine, the jsonic core and the fixture runner are path dependencies
on sibling checkouts, so there is nothing to fetch:

```bash
cargo test --all-targets
```

Or, from the repository root, `make test-rs`. For what CI would say,
including formatting and the lockfile check, run `ci/rust/run.sh`.

The suite runs every shared `../test/spec/*.tsv` fixture, the same files
the TypeScript and Go suites run, discovered by listing the directory so
a new fixture runs everywhere at once. Beside them are the divergence
register, the conformance corpus (both halves, and it never skips: if the
corpus cannot be fetched the suite fails), and the in-language tests for
what a fixture cannot express: the API surface, special floats,
multi-line strings, date kinds, the leading byte order mark, error
columns, key conflicts, the embedded grammar, and the shared default
parser under threads.

The conformance corpus is not committed. `../scripts/fetch-toml-test.sh`
clones it, pinned to an exact commit, and the suite runs that script
itself when the corpus is missing.

## License

MIT.
