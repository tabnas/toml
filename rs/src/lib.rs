// Copyright (c) 2021-2026 Richard Rodger, MIT License

// The engine's error carries a code, position, hint and a formatted
// report, so it is large by design and `Result<_, TabnasError>` trips
// clippy's `result_large_err`. The engine allows the lint at its own crate
// root for the same reason; boxing here would make `parse` return a
// different shape from `Tabnas::parse` and from the other two ports.
#![allow(clippy::result_large_err)]

//! The TOML grammar plugin for the `tabnas` parsing engine.
//!
//! It parses [TOML](https://toml.io) into plain values: bare, quoted and
//! dotted keys, `[table]` headers, `[[array-of-tables]]`, inline tables,
//! arrays, basic, literal and multi-line strings, integers and floats
//! including `inf` and `nan`, booleans, and date and time values.
//!
//! It is not standalone. It layers on the relaxed-JSON base grammar from
//! [`tabnas_jsonic`] (the `val` / `map` / `list` / `pair` / `elem` rules),
//! sets `rule.start` to `toml` and excludes the jsonic alternates, then
//! adds the TOML rules, a custom string matcher and context-aware date and
//! time matchers on top.
//!
//! ```
//! let value = tabnas_toml::parse("title = \"TOML\"\n[owner]\nname = \"Tom\"")?;
//! assert_eq!(
//!     value.to_string(),
//!     r#"{"title":"TOML","owner":{"name":"Tom"}}"#
//! );
//! # Ok::<(), tabnas_toml::TomlError>(())
//! ```
//!
//! TypeScript is canonical: `ts/src/toml.ts` defines behaviour, and the
//! grammar itself is authored once in `toml-grammar.jsonic` and embedded
//! into every runtime. The shared fixtures in `test/spec/*.tsv` are the
//! parity contract across TypeScript, Go and Rust.

use std::sync::OnceLock;

use serde_json::{json, Map, Value as Json};
use tabnas::{Context, GrammarError, GrammarSpec, Plugin, PluginError, Tabnas, Value, ValueDef};

mod datematcher;
mod daterange;
mod node;
mod refs;
mod strmatcher;
mod values;

pub use tabnas::TabnasError as TomlError;
pub use values::{toml_time, TomlTime, LOCAL_DATE, LOCAL_DATE_TIME, LOCAL_TIME, OFFSET_DATE_TIME};

/// This crate's version. It MUST equal `ts/package.json` "version": the
/// release orchestrator rewrites both, and `tests/version_test.rs` fails
/// the build if they drift. Mirrors `VERSION` in `ts/src/toml.ts` and
/// `const VERSION` in `go/toml.go`.
pub const VERSION: &str = "0.5.9";

/// The README's Rust examples run as doctests, so a stale one fails the
/// gate rather than misleading the reader. Its `toml` and `bash` fences
/// are skipped; rustdoc runs only the `rust` ones.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_examples {}

// --- BEGIN EMBEDDED toml-grammar.jsonic ---
pub(crate) const GRAMMAR_TEXT: &str = r#"
# TOML Grammar Definition
# Parsed by a standard Jsonic instance and passed to jsonic.grammar()
# Function references (@ prefixed) are resolved against the refs map.
# Regex references (@/pattern/flags) are resolved to RegExp instances.

{
  options: rule: { start: toml exclude: jsonic }
  options: lex: {
    emptyResult: {}
    match: string: make: '@make-toml-string-matcher'
  }
  options: fixed: token: { '#CL': '=' '#DOT': '.' }
  options: match: {
    token: { '#ID': '@/^[a-zA-Z0-9_-]+/' }
    value: {
      isodate: {
        match: '@/^\\d\\d\\d\\d-\\d\\d-\\d\\d([Tt ]\\d\\d:\\d\\d(:\\d\\d(\\.\\d+)?)?([Zz]|[-+]\\d\\d:\\d\\d)?)?/'
        val: '@isodate-val'
      }
      localtime: {
        match: '@/^\\d\\d:\\d\\d(:\\d\\d(\\.\\d+)?)?/'
        val: '@localtime-val'
      }
    }
  }
  options: tokenSet: {
    KEY: ['#ST' '#ID' null null]
  }
  options: comment: def: { slash: null multi: null }

  rule: toml: open: [
    { s: ['#ST #NR #ID' '#CL'] p: table b: 2 }
    { s: ['#OS' '#ST #NR #ID'] p: table b: 2 }
    { s: ['#OS' '#OS'] p: table b: 2 }
    { s: ['#ST #NR #ID' '#DOT'] p: table b: 2 }
    { s: '#ZZ' }
  ]

  rule: table: {
    open: [
      { s: ['#ST #NR #ID' '#CL'] p: map b: 2 }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 }
      { s: ['#OS' '#OS'] r: table n: { table_array: 1 } }
      {
        s: ['#ST #NR #ID' '#DOT']
        c: '@table-top-dive-cond'
        p: dive
        b: 2
        u: { top_dive: true }
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        c: '@lte-table-dive'
        n: { table_dive: 1 }
        a: '@table-dive-start'
        g: 'dive,start'
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        n: { table_dive: 1 }
        a: '@table-dive-mid'
        g: 'dive'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        c: '@lte-table-dive'
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-head'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-tail'
        g: 'dive,end'
      }
      {
        s: '#CS'
        p: map
        c: '@lte-table-array-1'
        a: '@table-cs-push'
      }
    ]
    close: [
      { s: ['#OS' '#OS'] r: table b: 2 g: end }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 g: end }
      { s: '#ZZ' g: end }
    ]
  }

  rule: map: {
    open: [
      { s: '#OS' b: 1 }
      {
        s: ['#ST #NR #ID' '#CL']
        c: '@map-is-table-parent'
        p: pair
        b: 2
      }
      { s: ['#OB' '#ST #NR #ID'] b: 1 p: pair }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
      { s: '#ZZ' }
    ]
    close: [
      { s: '#OS' b: 1 g: end }
      { s: '#ZZ' g: end }
    ]
  }

  rule: pair: {
    open: [
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        u: { pair: true }
        a: '@pair-key-set'
      }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
    ]
    close: [
      { s: ['#ST #NR #ID'] b: 1 r: pair g: comma }
      { s: ['#CA' '#ST #NR #ID'] b: 1 r: pair g: comma }
      { s: ['#OS'] b: 1 g: end }
      { s: ['#CA' '#CB'] c: '@lte-pk' b: 1 g: close }
    ]
  }

  rule: val: close: [
    { s: ['#ST #NR #ID'] b: 1 g: end }
    { s: ['#OS'] b: 1 g: end }
  ]

  rule: elem: close: [
    { s: ['#CA' '#CS'] b: 1 g: comma }
  ]

  # A dotted key is a replace loop, as a dotted header is: each segment
  # ending in a dot re-enters dive in the same frame (r), so rule depth
  # stays what one segment needs however long the key, and only the value
  # nests. dive_key counts the tables a key has descended through from the
  # table it is in; the loop at the close, which takes the next dotted key
  # without returning to the pair, resets it.
  rule: dive: {
    open: [
      {
        s: ['#ST #NR #ID' '#DOT']
        r: dive
        n: { dive_key: 1 }
        a: '@dive-key-dot'
      }
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        u: { dive_end: true }
      }
    ]
    close: [
      {
        s: ['#ST #NR #ID' '#DOT']
        b: 2
        r: dive
        n: { dive_key: 0 }
      }
      {}
    ]
  }
}
"#;
// --- END EMBEDDED toml-grammar.jsonic ---

/// The plugin's name, as the engine records it.
const PLUGIN_NAME: &str = "toml";

/// Parser options. Reserved for future use, as `TomlOptions` is in both
/// other ports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TomlOptions;

/// The lexer orders custom matchers against the builtin bands. All four of
/// these sit below the `match` band at 1e6, so they are consulted before
/// the value and token matchers, and the leading BOM before any of them.
const BOM_ORDER: f64 = 500_000.0;
const STRING_ORDER: f64 = 900_000.0;
const ISODATE_ORDER: f64 = 950_000.0;
const LOCALTIME_ORDER: f64 = 950_001.0;

/// Parse the embedded grammar with a standard jsonic instance, exactly as
/// the TypeScript and Go ports do, and answer it as a JSON document the
/// engine's grammar loader takes.
fn grammar_document() -> Result<Json, GrammarError> {
    let parsed = tabnas_jsonic::make()
        .parse(GRAMMAR_TEXT)
        .map_err(|error| GrammarError(format!("Grammar: parse grammar text: {error}")))?;
    let mut document = parsed.to_json();
    integerize(&mut document);
    let Some(root) = document.as_object_mut() else {
        return Err(GrammarError(
            "Grammar: the grammar text is not a map".into(),
        ));
    };
    let options = root
        .entry("options".to_string())
        .or_insert_with(|| Json::Object(Map::new()));
    let Some(options) = options.as_object_mut() else {
        return Err(GrammarError("Grammar: options is not a map".into()));
    };

    adjust_token_sets(options);
    adjust_value_matchers(options);
    adjust_lex_matchers(options);
    adjust_messages(options);

    Ok(document)
}

/// Every number in a jsonic parse result is an `f64`, so `b: 2` arrives as
/// `2.0` and the grammar loader, which wants a non-negative INTEGER for a
/// backtrack count, refuses it. Fold every fractionless float back to an
/// integer across the whole document. The Go port does the same, one field
/// at a time, in `mapToAlt`.
///
/// Nothing in a grammar document means a float where an integer would do:
/// the numeric fields are backtrack counts, counter deltas, rule depths
/// and matcher orders.
fn integerize(value: &mut Json) {
    match value {
        Json::Number(number) => {
            if let Some(float) = number.as_f64() {
                #[allow(clippy::cast_possible_truncation)]
                if float.is_finite() && 0.0 == float.fract() && float.abs() < 9.0e15 {
                    *value = Json::from(float as i64);
                }
            }
        }
        Json::Array(items) => items.iter_mut().for_each(integerize),
        Json::Object(entries) => entries.values_mut().for_each(integerize),
        _ => {}
    }
}

/// `tokenSet: { KEY: ['#ST' '#ID' null null] }`. The canonical token-set
/// form pads a set to four slots with nulls; this engine's loader takes
/// the members alone and rejects a null entry, so the padding is dropped.
/// Same set either way: a null slot names no token.
fn adjust_token_sets(options: &mut Map<String, Json>) {
    let Some(sets) = options.get_mut("tokenSet").and_then(Json::as_object_mut) else {
        return;
    };
    for members in sets.values_mut() {
        if let Some(list) = members.as_array_mut() {
            list.retain(|member| member.is_string());
        }
    }
}

/// Respell the grammar's `match.value` date and time patterns with ASCII
/// digit classes.
///
/// The shared grammar text writes them with `\d`, which is right for the
/// two runtimes that read it: JavaScript compiles them without the `u`
/// flag and Go's RE2 has no Unicode `\d` at all, so in both `\d` is
/// `0-9`. The `regex` crate reads `\d` as the whole Unicode `Nd`
/// category, so the same text compiled here matches `٢٠٢٤-٠١-٠١` and
/// turns it into a `local-date` value, where both other runtimes leave it
/// as ordinary text.
///
/// The canonical port replaces these two matchers outright once the
/// document is in, and so, in effect, does this one: the native matchers
/// in [`datematcher`] are ordered ahead of the value band. But the
/// grammar's own declarations stay installed here, and a value-band
/// matcher is still consulted when the native one declines, so they have
/// to agree with it rather than merely be shadowed by it. Taking the
/// pattern from the native matcher itself is what keeps the two in step.
fn adjust_value_matchers(options: &mut Map<String, Json>) {
    let Some(values) = options
        .get_mut("match")
        .and_then(Json::as_object_mut)
        .and_then(|matchers| matchers.get_mut("value"))
        .and_then(Json::as_object_mut)
    else {
        return;
    };
    for (name, ascii) in [
        ("isodate", datematcher::isodate_re().as_str()),
        ("localtime", datematcher::localtime_re().as_str()),
    ] {
        let Some(entry) = values.get_mut(name).and_then(Json::as_object_mut) else {
            continue;
        };
        // Only a regex reference is respelled. A `@name` function
        // reference means the grammar has moved on and the substitution
        // would no longer be describing the same thing.
        if entry
            .get("match")
            .and_then(Json::as_str)
            .is_some_and(|reference| reference.starts_with("@/"))
        {
            entry.insert("match".to_string(), json!(format!("@/{ascii}/")));
        }
    }
}

/// The grammar declares `lex.match.string.make` with no order, because the
/// canonical engine's string matcher is a named builtin that this
/// REPLACES. Here the builtin bands are fixed and a custom matcher is
/// placed among them by order, so the string matcher is given one that
/// puts it ahead of every band that could claim a quote.
///
/// The BOM and the two date matchers are added here rather than to the
/// grammar text, because the text is shared with two ports that install
/// their own equivalents natively.
fn adjust_lex_matchers(options: &mut Map<String, Json>) {
    let lex = options
        .entry("lex".to_string())
        .or_insert_with(|| Json::Object(Map::new()));
    let Some(lex) = lex.as_object_mut() else {
        return;
    };
    let matchers = lex
        .entry("match".to_string())
        .or_insert_with(|| Json::Object(Map::new()));
    let Some(matchers) = matchers.as_object_mut() else {
        return;
    };

    if let Some(string) = matchers.get_mut("string").and_then(Json::as_object_mut) {
        string.insert("order".to_string(), json!(STRING_ORDER));
    }
    matchers.insert(
        "bom".to_string(),
        json!({ "order": BOM_ORDER, "make": "@bom-matcher" }),
    );
    matchers.insert(
        "tomlisodate".to_string(),
        json!({ "order": ISODATE_ORDER, "make": "@isodate-match" }),
    );
    matchers.insert(
        "tomllocaltime".to_string(),
        json!({ "order": LOCALTIME_ORDER, "make": "@localtime-match" }),
    );
}

/// A code with no template renders as "unknown error: toml_key_conflict",
/// which tells the author nothing. Kept in step with the TypeScript port's
/// registration, so a document rejected by both is rejected in the same
/// words.
fn adjust_messages(options: &mut Map<String, Json>) {
    options.insert(
        "error".to_string(),
        json!({
            "toml_key_conflict": "cannot define {key}, {why}",
            "invalid_datetime": "date or time is out of range",
        }),
    );
    options.insert(
        "hint".to_string(),
        json!({
            "toml_key_conflict": "\nTOML does not allow a key to be redefined, and a key that already holds a\nvalue is not a table you can add to. This usually means the same name was\nused twice - as a value and then as a table or table-array header, or twice\ninside one inline table.",
            "invalid_datetime": "\nThe value has the shape of a date or time, but one of its components is out\nof range: month 1-12, day 1 to the length of that month, hour 0-23, minute\nand second 0-59 (a second may be 60, for a leap second), and the same limits\nagain for a +hh:mm offset. February is checked against the actual year, so\n2100-02-29 is rejected - 2100 is not a leap year.",
        }),
    );
}

/// How many levels a document may nest before the parse is refused, with
/// the engine's `cancel` code: 127 levels parse, the 128th is refused.
///
/// A level is a container the value being built sits in: the document's
/// root table, a table or an array of tables along a header's or a dotted
/// key's path, an inline table, an array. The number is jsonic's, which
/// `tabnas-json` and `serde_json` use too, so an inline table, which is
/// jsonic's, is bounded where it always was, and a dotted key and a
/// header, which nest through this grammar's own rules and went unbounded
/// under jsonic's check, are bounded with it (tabnas/toml#78): a key of
/// 10,000 segments used to parse, building a value 10,000 deep.
///
/// The engine parses iteratively, but displaying, converting or dropping a
/// `Value` walks the tree with the call stack, so an unbounded document
/// ends the caller's process rather than returning an error. TypeScript
/// and Go have no limit, which `../test/divergent.tsv` records. A caller
/// that wants the depth anyway lifts the guard with
/// `parser.remove_parse_guard(DEPTH_GUARD)`, and drops the value on a
/// stack that can take it.
pub const DEPTH_LIMIT: usize = 127;

/// The name the depth check is installed under, as a parse guard, and the
/// name `Tabnas::remove_parse_guard` takes to lift it.
///
/// It is the name jsonic installs its own check under, so this one
/// replaces it, as YAML's does: jsonic's counts its `map` and `list`
/// rules, which is every inline table and array, but a header and a
/// dotted key nest through `table` and `dive`, which it does not see. A
/// guard rather than the parse budget, because the budget is one slot a
/// caller's `parse_budget` replaces, and a guard holds whatever budget the
/// caller sets. A parse a grammar's guard cancels is reported by the
/// fleet's tools as the input's fault, naming the guard.
pub const DEPTH_GUARD: &str = "depth";

/// Whether a rule of this name holds a container: jsonic's `map` and
/// `list`, which here are a table's body, an inline table and an array.
fn is_container(name: &str) -> bool {
    "map" == name || "list" == name
}

/// How deep the value being built is nested at this point of the parse:
/// the root table; the containers open on the rule stack and in the rule
/// the loop is working on, which the engine hands over apart from the
/// stack; the tables the nearest header reached; and the tables the
/// current dotted key has descended through, the `dive_key` counter the
/// grammar keeps on the dive and every rule under it inherits, an inline
/// table's keys included.
///
/// The root table is counted first, because no rule stands for it on its
/// own: a document's keys live in a `map` rule the root `table` pushes, or
/// in a `dive` it pushes straight away when the first key is dotted. A
/// header's tables are read two ways. A `table` rule on the stack has its
/// body open, and its path, kept in its `u` bag, is exact, arrays of
/// tables included; the body itself is a `map` rule INSIDE the table at
/// the end of that path, not a level of its own, so the `map` directly
/// above such a table is taken off again (and the root's body with it,
/// at path length zero). A `table` rule that is the current rule is still
/// reading its header, one segment per replacement, and a replacement
/// inherits counters but not `u`, so its path says nothing yet: there the
/// `table_dive` counter, the segments entered so far, plus the one being
/// read, say how deep the header has got, so a header is refused at the
/// segment that passes the limit and never builds a value past it.
///
/// Counted afresh at each step. Every level holds a container and a few
/// other rules, and a repetition re-enters its rule in one frame, so what
/// a step walks is bounded by the limit itself.
fn depth(context: &Context) -> usize {
    let counter = |rule: &tabnas::RuleSnapshot, name: &str| {
        rule.n
            .get(name)
            .map_or(0, |count| usize::try_from(*count).unwrap_or(0))
    };
    let current = context.rule.as_ref();
    let mut levels = 1;
    let mut header_seen = false;
    let mut above: Option<&str> = None;
    let rules = current.into_iter().chain(context.rule_stack.iter().rev());
    for (index, rule) in rules.enumerate() {
        let name: &str = rule.name.as_ref();
        if is_container(name) {
            levels += 1;
        } else if "table" == name && !header_seen {
            header_seen = true;
            if 0 == index && current.is_some() {
                levels += 1 + counter(rule, "table_dive");
            } else {
                levels += node::snapshot_path(Some(rule)).len();
                if Some("map") == above {
                    levels -= 1;
                }
            }
        }
        above = Some(name);
    }
    levels + current.map_or(0, |rule| counter(rule, "dive_key"))
}

/// The depth guard: [`DEPTH_LIMIT`] levels parse, the next one is refused.
fn within_depth_limit(context: &Context) -> bool {
    depth(context) <= DEPTH_LIMIT
}

/// Install the TOML grammar on an instance that already carries the
/// jsonic base grammar.
///
/// ```
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut parser = tabnas_jsonic::make();
///     tabnas_toml::toml(&mut parser)?;
///     assert_eq!(parser.parse("a = 1")?.to_string(), r#"{"a":1}"#);
///     Ok(())
/// }
/// ```
pub fn toml(parser: &mut Tabnas) -> Result<(), GrammarError> {
    refs::register(parser);
    register_native_matchers(parser);

    let document = grammar_document()?;
    let spec = GrammarSpec::from_value(document)?;
    parser.grammar(&spec)?;

    // Replaces jsonic's guard, which counts only jsonic's own containers
    // (see `DEPTH_GUARD`).
    parser.parse_guard(DEPTH_GUARD, within_depth_limit);

    // After the document, because it is the document that installs the
    // option tree these replace part of.
    register_special_floats(parser)?;
    Ok(())
}

/// The three matchers this port implements natively rather than in the
/// shared grammar text.
fn register_native_matchers(parser: &mut Tabnas) {
    parser.imperative_lex_match_ref("@bom-matcher", datematcher::bom_matcher);
    parser.imperative_lex_match_ref("@isodate-match", datematcher::isodate_matcher);
    parser.imperative_lex_match_ref("@localtime-match", datematcher::localtime_matcher);
}

/// TOML's `nan` and `inf` keyword values, added alongside the standard
/// `true` / `false` / `null`.
///
/// Not in the grammar document, for two reasons. The literals cannot
/// round-trip through a jsonic parse of the grammar text: they come back
/// as the strings. And a keyword definition takes a literal `val`, never a
/// function reference, so there is no `@`-name for a number JSON cannot
/// spell. Both other ports patch them in code for the same reason.
/// Assigning the definitions REPLACES the engine's defaults, so
/// `true` / `false` / `null` are restated.
fn register_special_floats(parser: &mut Tabnas) -> Result<(), GrammarError> {
    parser
        .set_options(|options| {
            let definitions = &mut options.value.definitions;
            definitions.clear();
            for (source, value) in [
                ("true", Value::Bool(true)),
                ("false", Value::Bool(false)),
                ("null", Value::Null),
                ("nan", Value::Number(f64::NAN)),
                ("+nan", Value::Number(f64::NAN)),
                ("-nan", Value::Number(f64::NAN)),
                ("inf", Value::Number(f64::INFINITY)),
                ("+inf", Value::Number(f64::INFINITY)),
                ("-inf", Value::Number(f64::NEG_INFINITY)),
            ] {
                definitions.insert(
                    source.to_string(),
                    ValueDef {
                        val: Some(value),
                        matcher: None,
                        transform: None,
                        consume: false,
                    },
                );
            }
        })
        .map_err(|error| GrammarError(format!("Grammar: value definitions: {error}")))?;
    Ok(())
}

/// The plugin form of [`toml`], for [`Tabnas::use_plugin`]. Installed this
/// way the grammar is re-applied to derived instances, as every native
/// plugin is.
pub fn plugin() -> Plugin {
    Plugin::new(PLUGIN_NAME, |parser, _options| {
        toml(parser).map_err(|error| PluginError(error.0))
    })
    .with_defaults(Value::object(indexmap::IndexMap::new()))
}

/// Build a TOML parser: a jsonic instance with this plugin installed, the
/// counterpart of `new Tabnas().use(jsonic).use(Toml)` and the Go
/// `MakeJsonic()`.
///
/// ```
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let parser = tabnas_toml::make();
///     assert_eq!(parser.parse("[a]\nx = 1")?.to_string(), r#"{"a":{"x":1}}"#);
///     Ok(())
/// }
/// ```
pub fn make() -> Tabnas {
    make_with(TomlOptions)
}

/// Build a TOML parser with plugin options. `TomlOptions` carries nothing
/// yet, in every port; the entry point exists so that adding one is not a
/// breaking change.
pub fn make_with(_options: TomlOptions) -> Tabnas {
    let mut parser = tabnas_jsonic::make();
    parser
        .use_plugin(plugin(), None)
        .expect("the toml grammar document is fixed and valid");
    parser
}

/// Parse a TOML document with a shared default parser.
///
/// Building the grammar dominates a parse, so the no-options path reuses
/// one instance. Parsing builds a fresh context per call and only reads
/// instance state, so the shared instance is safe for concurrent use.
///
/// ```
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let value = tabnas_toml::parse("[[products]]\nname = \"Hammer\"")?;
///     assert_eq!(value.to_string(), r#"{"products":[{"name":"Hammer"}]}"#);
///     Ok(())
/// }
/// ```
pub fn parse(src: &str) -> Result<Value, TomlError> {
    static DEFAULT: OnceLock<Tabnas> = OnceLock::new();
    DEFAULT.get_or_init(make).parse(src)
}

/// The grammar text this crate embeds, for the test that compares it with
/// the file on disk.
#[doc(hidden)]
pub fn grammar_text() -> &'static str {
    GRAMMAR_TEXT
}

/// One optional alchemy translation source and its explicit entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranslationPart {
    /// The definition a host calls after linking the source.
    pub entry: &'static str,
    /// The source text, or `None` for an entry supplied by alchemy.
    pub source: Option<&'static str>,
}

/// The package-local structural translation interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranslationParts {
    /// The complete `tabnas.plugin.json` text.
    pub manifest: &'static str,
    /// An optional lift from the grammar's events to its first read shape.
    pub lift: Option<TranslationPart>,
    /// An optional render from the write shape to text.
    pub render: Option<TranslationPart>,
}

const TRANSLATION: TranslationParts = TranslationParts {
    manifest: include_str!("../translate/manifest.json"),
    lift: None,
    render: Some(TranslationPart {
        entry: "toml-render",
        source: Some(include_str!("../translate/render.alc")),
    }),
};

/// Return TOML's immutable translation parts.
#[must_use]
pub const fn translate() -> Option<TranslationParts> {
    Some(TRANSLATION)
}

/// The plugin's manifest, `tabnas.plugin.json`, as the repository carries
/// it. Its `translate` object is what a host that translates reads: the
/// shape TOML is read as and written from (`tree`), the file that holds
/// the render, and the sentences that say what the render does not keep.
/// The crate embeds its own copy, `translate/manifest.json`, since a
/// packaged crate holds nothing outside `rs/`; `tests/translate_test.rs`
/// holds the copy to the file.
///
/// ```
/// assert!(tabnas_toml::manifest_text().contains("\"translate\""));
/// ```
pub fn manifest_text() -> &'static str {
    TRANSLATION.manifest
}

/// TOML's render, `alchemy/render.alc`, the file the manifest's
/// `translate.render` names: a library of alchemy definitions, with no
/// `export`, whose entry point `toml-render` writes a tree's events as one
/// TOML document. A host links it with its own program. The crate embeds
/// its own copy, `translate/render.alc`, held to the file as the
/// manifest's is.
///
/// ```
/// assert!(tabnas_toml::render_text().contains("def toml-render [input]"));
/// ```
pub fn render_text() -> &'static str {
    match TRANSLATION.render {
        Some(part) => part.source.unwrap_or_default(),
        None => "",
    }
}
