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

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use serde_json::{json, Map, Value as Json};
use tabnas::{
    Context, GrammarError, GrammarSpec, Plugin, PluginError, RuleSnapshot, RuleState, Tabnas,
    Value, ValueDef,
};

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
pub const VERSION: &str = "0.5.16";

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
  # nests. The loop at the close takes the next dotted key without
  # returning to the pair, and begins it from the table that holds both.
  rule: dive: {
    open: [
      {
        s: ['#ST #NR #ID' '#DOT']
        r: dive
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
            "toml_key_conflict": "\nTOML does not allow a key to be redefined, and a key that already holds a\nvalue is not a table you can add to. This usually means the same name was\nused twice: a key given a second value, a table or table-array header over a\nkey that already holds a value, a header written a second time, a header for\na table that a dotted key or an inline table had already defined, or the same\nname twice inside one inline table.",
            "invalid_datetime": "\nThe value has the shape of a date or time, but one of its components is out\nof range: month 1-12, day 1 to the length of that month, hour 0-23, minute\nand second 0-59 (a second may be 60, for a leap second), and the same limits\nagain for a +hh:mm offset. February is checked against the actual year, so\n2100-02-29 is rejected - 2100 is not a leap year.",
        }),
    );
}

/// How many levels a document may nest before the parse is refused, with
/// the engine's `cancel` code: 127 levels parse, the 128th is refused.
///
/// A level is a container the value being built sits in: the document's
/// root table, a table or an array of tables along a header's or a dotted
/// key's path, an inline table, an array. Every one counts, however the
/// parse reaches it, and a key inside an inline table counts on from the
/// keys outside it. The number is jsonic's, which `tabnas-json` and
/// `serde_json` use too. jsonic's own check counts its `map` and `list`
/// rules, which here are every inline table, every array and the body of
/// every table, and never the tables a header or a dotted key nests
/// through: under it a key of 10,000 segments parsed, building a value
/// 10,000 deep (tabnas/toml#78). So an inline table or an array in the
/// root table is bounded where jsonic's check bounded it, and one under a
/// header or a dotted key now has that header's or key's tables counted
/// above it.
///
/// The engine parses iteratively, but displaying, converting or dropping a
/// `Value` walks the tree with the call stack, so an unbounded document
/// ends the caller's process rather than returning an error. TypeScript
/// and Go have no limit, which `../test/divergent.tsv` records.
///
/// A caller that wants the depth anyway lifts the guard with
/// `parser.remove_parse_guard(DEPTH_GUARD)`. That lifts every bound, and
/// not only the one this guard adds: the guard replaced jsonic's check
/// under the same name, so nothing is left to stop an inline table or an
/// array either, and a document of any depth parses. The caller then
/// displays and drops the value on a stack that can take it.
pub const DEPTH_LIMIT: usize = 127;

/// The name the depth check is installed under, as a parse guard, and the
/// name `Tabnas::remove_parse_guard` takes to lift it.
///
/// It is the name jsonic installs its own check under, so this one
/// replaces it, as YAML's does: jsonic's counts its `map` and `list`
/// rules, which is every inline table and array, but a header and a
/// dotted key nest through `table` and `dive`, which it does not see. So
/// removing this guard removes jsonic's bound with it, and leaves none
/// (see [`DEPTH_LIMIT`]). A guard rather than the parse budget, because
/// the budget is one slot a caller's `parse_budget` replaces, and a guard
/// holds whatever budget the caller sets. A parse a grammar's guard
/// cancels is reported by the fleet's tools as the input's fault, naming
/// the guard.
pub const DEPTH_GUARD: &str = "depth";

/// How deep the value being built is nested at this point of the parse:
/// the root table, what the rules on the rule stack add to it
/// ([`levels_on_stack`]), and what the rule the loop is working on adds
/// ([`current_levels`]).
///
/// The root table is counted first, because no rule stands for it on its
/// own: a document's keys live in a `map` rule the root `table` pushes, or
/// in a `dive` it pushes straight away when the first key is dotted.
fn depth(context: &Context) -> usize {
    let below = context.rule_stack.last();
    let current = context
        .rule
        .as_ref()
        .map_or(0, |rule| current_levels(rule, below));
    1 + levels_on_stack(context) + current
}

/// The levels the rules on the rule stack add, carried from one step to
/// the next.
///
/// The guard runs at every step, and walking the whole stack each time
/// cost a step as much as the stack is deep: 2,000 lines of arrays or
/// inline tables nested 120 deep took 20 to 28 percent longer to parse
/// than under jsonic's guard, which keeps its count this way
/// (tabnas/jsonic#91). The engine changes the stack only at the top: a
/// pop truncates it and a push appends the new rule's snapshot, and the
/// rules below the top stay as they were (the engine checks this in
/// debug builds). What a frame adds depends on it and on the frame below
/// it alone ([`frame_levels`]), and a rule's ancestors are fixed for as
/// long as it lives, so the levels at or below a stack position are known
/// once the rule at that position is. The count is kept for each position
/// with the id of the rule there, which is unique within a parse, and a
/// step recounts only from the first position whose rule has changed,
/// usually the top. A callback that writes into `context.rule_stack`
/// below the top is not followed.
fn levels_on_stack(context: &Context) -> usize {
    let stack = &context.rule_stack;
    STACK_COUNT.with(|count| {
        let mut count = count.borrow_mut();
        let here = std::ptr::from_ref(context) as usize;
        if count.context != here || 1 == context.iteration || context.iteration < count.iteration {
            count.context = here;
            count.frames.clear();
        }
        count.iteration = context.iteration;
        let top = count.frames.len().min(stack.len());
        let mut kept = top;
        while kept > 0 && count.frames[kept - 1].0 != stack[kept - 1].i {
            kept -= 1;
        }
        #[cfg(test)]
        {
            count.examined += (top - kept) + 1 + (stack.len() - kept);
        }
        count.frames.truncate(kept);
        let mut levels = count.frames.last().map_or(0, |&(_, at)| at);
        for position in kept..stack.len() {
            let below = position.checked_sub(1).map(|under| &stack[under]);
            levels += frame_levels(&stack[position], below);
            count.frames.push((stack[position].i, levels));
        }
        levels
    })
}

/// The count [`levels_on_stack`] keeps from one step to the next.
struct StackCount {
    /// The context the count describes, by address.
    context: usize,
    /// The last step counted. A parse's steps go up from 1, so a first
    /// step, or one lower than the last, is a new parse in the same place;
    /// the same step again is the same step, asked twice.
    iteration: usize,
    /// For each stack position, the id of the rule there and the levels
    /// the rules at or below it add.
    frames: Vec<(usize, usize)>,
    /// Frames looked at, all told: what a step costs, for the tests.
    #[cfg(test)]
    examined: usize,
}

thread_local! {
    // Per thread, as a parse runs on one; a nested parse on the same
    // thread has a context of its own and starts the count afresh.
    static STACK_COUNT: RefCell<StackCount> = const {
        RefCell::new(StackCount {
            context: 0,
            iteration: 0,
            frames: Vec::new(),
            #[cfg(test)]
            examined: 0,
        })
    };
}

/// The levels a rule on the rule stack adds, given the rule below it.
///
/// A `map` or `list` is a container: a table's body, an inline table, an
/// array. A `table` on the stack has its body open, and its path, kept in
/// its `u` bag, is exact, arrays of tables included; its body is a `map`
/// INSIDE the table at the end of that path, not a level of its own, so a
/// `map` directly above a `table` adds nothing (the root's body included,
/// at path length zero).
///
/// A `dive` on the stack is a dotted key waiting for its value, and its
/// path counts the tables the key has descended through from the table it
/// is in: the cell of the table body or inline table that holds the key,
/// or the root table's when the root table pushed the dive itself. So
/// every key still open counts, and a key in an inline table that is the
/// value of another key counts on from that key, however the parse
/// reached it. A counter cannot say the same: the dive's close loop takes
/// the next dotted key in the same frame and has to begin it from the
/// table it is in, and a counter the loop reset lost the tables of every
/// key further out with it. A dotted key on a new line of an inline table
/// once counted from zero that way.
fn frame_levels(rule: &Rc<RuleSnapshot>, below: Option<&Rc<RuleSnapshot>>) -> usize {
    match rule.name.as_ref() {
        "map" => usize::from(below.is_none_or(|below| "table" != below.name.as_ref())),
        "list" => 1,
        "table" | "dive" => node::snapshot_path(Some(rule)).len(),
        _ => 0,
    }
}

/// The levels the rule the loop is working on adds, given the top of the
/// rule stack. The engine hands that rule over apart from the stack, and
/// at the start of its step, before any of its actions have run.
///
/// A `table` that is the current rule is still reading its header, one
/// segment per replacement, and a replacement inherits counters but not
/// `u`, so its path says nothing yet: there the `table_dive` counter, the
/// segments entered so far, plus the one being read, say how deep the
/// header has got, so a header is refused at the segment that passes the
/// limit and never builds a value past it. A `dive` that is still open
/// has not run `@dive-bo`, which records its path, yet: it stands where
/// the segment it replaced left off when it continues a key, and has
/// descended through nothing when it begins one. So a dotted key is
/// refused at the segment past the limit too.
fn current_levels(rule: &Rc<RuleSnapshot>, below: Option<&Rc<RuleSnapshot>>) -> usize {
    match rule.name.as_ref() {
        "table" => {
            let entered = rule.n.get("table_dive").copied().unwrap_or(0);
            1 + usize::try_from(entered).unwrap_or(0)
        }
        "dive" if RuleState::Open == rule.state => {
            let previous = rule.prev_rule.as_ref();
            if refs::continues_key(previous) {
                node::snapshot_path(previous).len()
            } else {
                0
            }
        }
        _ => frame_levels(rule, below),
    }
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
    /// An optional embedding of a plain tree in the format's schema, with its reverse.
    pub embed: Option<TranslationPart>,
    /// An optional render from the write shape to text.
    pub render: Option<TranslationPart>,
}

const TRANSLATION: TranslationParts = TranslationParts {
    manifest: include_str!("../translate/manifest.json"),
    lift: None,
    embed: None,
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    /// [`depth`] with the stack walked afresh, as it was counted before
    /// the count was kept from step to step.
    fn walked(context: &Context) -> usize {
        let mut levels = 1;
        let mut below = None;
        for rule in &context.rule_stack {
            levels += frame_levels(rule, below);
            below = Some(rule);
        }
        let current = context
            .rule
            .as_ref()
            .map_or(0, |rule| current_levels(rule, below));
        levels + current
    }

    fn segments(n: usize) -> String {
        vec!["a"; n].join(".")
    }

    #[test]
    fn the_kept_count_is_the_walked_count_at_every_step() {
        // The engine turns a panic in a guard into a parse error, so a
        // difference is written down and asserted after the parses. The
        // check replaces the crate's own guard, so the count is kept once
        // per step, as it is in use.
        let differences = Arc::new(Mutex::new(Vec::new()));
        let steps = Arc::new(AtomicUsize::new(0));
        let (seen, stepped) = (differences.clone(), steps.clone());
        let mut parser = make();
        parser.parse_guard(DEPTH_GUARD, move |context| {
            let (kept, walked) = (depth(context), walked(context));
            if kept != walked {
                seen.lock().unwrap().push((context.iteration, kept, walked));
            }
            stepped.fetch_add(1, Ordering::Relaxed);
            kept <= DEPTH_LIMIT
        });
        let nested = |open: &str, inner: &str, close: &str, n: usize| {
            format!("{}{inner}{}", open.repeat(n), close.repeat(n))
        };
        let sources = [
            "a = 1".to_string(),
            "a = 1\nb.c = 2\nd.e.f = 3".to_string(),
            "a.b = 1\nc.d = 2".to_string(),
            "[a.b]\nc = 1\n[[d.e]]\nf = {g = [1, {h.i = 2}]}\n[[d.e]]\nj.k = 3".to_string(),
            "[[a]]\n[[a.b]]\n[[a.b.c]]\nx = 1".to_string(),
            "k.k = {p.q = 1\nr.s = {t.u = 2\nv.w = 3}}".to_string(),
            "x = [{p.q = 1\na.b.c = 2}, [[3]]]".to_string(),
            "a = \"unterminated".to_string(),
            "a = 1\n[a]".to_string(),
            "a = ]".to_string(),
            format!("{} = 1", segments(127)),
            format!("{} = 1", segments(128)),
            format!("[{}]\nx = 1", segments(127)),
            format!("x = {}", nested("[", "", "]", 127)),
            format!("x = {}", nested("{a = ", "1", "}", 130)),
            format!("{} = {{p.q = 1\n{} = 1}}", segments(100), segments(28)),
            nested(&format!("{} = {{p.q = 1\n", segments(25)), "z = 1", "}", 6),
        ];
        // Twice over, so each parse also follows one that ended, some of
        // them in an error, on the same thread.
        for source in sources.iter().chain(sources.iter()) {
            let _ = parser.parse(source);
        }
        let steps = steps.load(Ordering::Relaxed);
        assert!(steps > 3_000, "{steps} steps");
        assert_eq!(*differences.lock().unwrap(), Vec::new());
    }

    #[test]
    fn a_step_counts_what_changed_not_the_whole_stack() {
        // Arrays 120 deep put about 360 rules on the stack: a `val`, a
        // `list` and an `elem` a level. Counting all of them at every
        // step looked at a hundred frames a step or more, and the kept
        // count looks at one or two. The work is counted rather than
        // timed: in a debug build the engine checks every buried frame at
        // every step, which would swamp a timing.
        let steps = Arc::new(AtomicUsize::new(0));
        let walk = Arc::new(AtomicUsize::new(0));
        let (stepped, walked) = (steps.clone(), walk.clone());
        let mut parser = make();
        parser.parse_guard(DEPTH_GUARD, move |context| {
            stepped.fetch_add(1, Ordering::Relaxed);
            walked.fetch_add(context.rule_stack.len(), Ordering::Relaxed);
            within_depth_limit(context)
        });
        let levels = 120;
        let src = format!("x = {}1{}", "[".repeat(levels), "]".repeat(levels));
        let examined = || STACK_COUNT.with(|count| count.borrow().examined);
        let before = examined();
        parser.parse(&src).expect("parses");
        let work = examined() - before;
        let steps = steps.load(Ordering::Relaxed);
        let walk = walk.load(Ordering::Relaxed);
        assert!(steps > 2 * levels, "{steps} steps");
        assert!(
            walk > levels / 2 * steps,
            "a shallow stack: {walk} frames in {steps} steps"
        );
        assert!(
            work <= 3 * steps,
            "{work} frames looked at in {steps} steps"
        );
    }
}
