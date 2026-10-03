// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

// In-language behaviour the shared fixtures cannot express: the API
// surface, the typed date value, error columns, the leading BOM, the
// embedded grammar, and the shared default parser under threads.
//
// Ports go/toml_test.go, go/features_test.go, go/strmatcher_col_test.go
// and ts/test/prototype-pollution.test.ts.

mod common;

use std::sync::{Arc, Mutex};
use std::thread;

use tabnas::{Tabnas, Value};
use tabnas_toml::{
    make, parse, plugin, toml, toml_time, TomlOptions, DEPTH_GUARD, DEPTH_LIMIT, LOCAL_DATE,
    LOCAL_DATE_TIME, LOCAL_TIME, OFFSET_DATE_TIME, VERSION,
};

use common::repo_dir;

fn field(src: &str, key: &str) -> Value {
    let value = parse(src).unwrap_or_else(|error| panic!("parse {src:?}: {error}"));
    let Value::Object(entries) = &value else {
        panic!("parse {src:?}: expected a map, got {value}");
    };
    entries
        .get(key)
        .cloned()
        .unwrap_or_else(|| panic!("parse {src:?}: no key {key:?} in {value}"))
}

// --- the API ------------------------------------------------------------

#[test]
fn parse_happy() {
    assert_eq!(Value::Number(1.0), field("a=1", "a"));
}

#[test]
fn parse_empty() {
    // `lex.emptyResult` is `{}`, so an empty document is an empty table,
    // not `undefined`.
    assert_eq!("{}", parse("").expect("empty parses").to_string());
}

#[test]
fn version_is_set() {
    assert!(!VERSION.is_empty());
}

#[test]
fn make_builds_an_independent_instance() {
    let first = make();
    let second = make();
    assert_eq!(r#"{"a":1}"#, first.parse("a = 1").expect("a").to_string());
    assert_eq!(r#"{"b":2}"#, second.parse("b = 2").expect("b").to_string());
}

#[test]
fn make_with_takes_the_reserved_options() {
    let parser = tabnas_toml::make_with(TomlOptions);
    assert_eq!(r#"{"a":1}"#, parser.parse("a = 1").expect("a").to_string());
}

#[test]
fn the_plugin_installs_on_a_jsonic_instance() {
    let mut parser = tabnas_jsonic::make();
    parser
        .use_plugin(plugin(), None)
        .expect("the plugin installs");
    assert_eq!(
        r#"{"a":{"x":1}}"#,
        parser.parse("[a]\nx = 1").expect("a table").to_string()
    );
}

#[test]
fn the_grammar_installs_directly_too() {
    let mut parser = tabnas_jsonic::make();
    toml(&mut parser).expect("the grammar installs");
    assert_eq!(r#"{"a":1}"#, parser.parse("a = 1").expect("a").to_string());
}

/// The shared default parser is read-only during a parse, so it may be
/// used from several threads at once. `parse` builds it once.
#[test]
fn parse_is_usable_from_many_threads() {
    let handles: Vec<_> = (0..8)
        .map(|index| {
            thread::spawn(move || {
                let src = format!("a = {index}\n[t]\nb = \"x\"");
                let value = parse(&src).expect("a parse per thread");
                assert_eq!(
                    format!(r#"{{"a":{index},"t":{{"b":"x"}}}}"#),
                    value.to_string()
                );
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("no thread panicked");
    }
}

// --- values -------------------------------------------------------------

#[test]
fn special_floats() {
    for (src, check) in [
        ("a = nan", "nan"),
        ("a = +nan", "nan"),
        ("a = -nan", "nan"),
        ("a = inf", "+inf"),
        ("a = +inf", "+inf"),
        ("a = -inf", "-inf"),
    ] {
        let Value::Number(number) = field(src, "a") else {
            panic!("{src}: not a number");
        };
        match check {
            "nan" => assert!(number.is_nan(), "{src}: {number}"),
            "+inf" => assert_eq!(f64::INFINITY, number, "{src}"),
            _ => assert_eq!(f64::NEG_INFINITY, number, "{src}"),
        }
    }
    assert_eq!(Value::Bool(true), field("a = true", "a"));
    assert_eq!(Value::Bool(false), field("a = false", "a"));
}

#[test]
fn triple_quoted() {
    for (src, want) in [
        (r#"a = """hello""""#, "hello"),
        (r#"a = """"hello"""""#, r#""hello""#),
        ("a = '''hello'''", "hello"),
        ("a = \"\"\"a\nb\"\"\"", "a\nb"),
    ] {
        assert_eq!(Value::String(want.to_string()), field(src, "a"), "{src}");
    }
}

#[test]
fn datetime_kinds() {
    for (src, want) in [
        ("a = 1979-05-27", LOCAL_DATE),
        ("a = 1979-05-27T07:32:00", LOCAL_DATE_TIME),
        ("a = 1979-05-27T07:32:00Z", OFFSET_DATE_TIME),
        ("a = 07:32:00", LOCAL_TIME),
    ] {
        let value = field(src, "a");
        let time = toml_time(&value)
            .unwrap_or_else(|| panic!("{src}: {value} is not a TOML date or time"));
        assert_eq!(want, time.kind, "{src}");
        assert_eq!(
            src.trim_start_matches("a = "),
            time.src,
            "{src}: source text"
        );
    }
}

/// A date value renders as its source text, which is what makes it
/// readable in a JSON dump.
#[test]
fn a_datetime_serializes_as_its_source() {
    assert_eq!(
        r#"{"a":"1979-05-27T07:32:00Z"}"#,
        parse("a = 1979-05-27T07:32:00Z")
            .expect("a datetime")
            .to_json()
            .to_string()
    );
}

/// A value that is not a date is not read as one, so `toml_time` cannot
/// be fooled by an ordinary string.
#[test]
fn toml_time_rejects_an_ordinary_value() {
    assert!(toml_time(&field(r#"a = "1979-05-27""#, "a")).is_none());
    assert!(toml_time(&Value::String("x".into())).is_none());
}

/// Only ASCII digits make a date or a time.
///
/// The `regex` crate reads `\d` as the whole Unicode `Nd` category, while
/// the JavaScript patterns this port comes from are compiled without the
/// `u` flag and the Go ones are RE2, so in both of those `\d` is `0-9`.
/// Every date and time pattern here is therefore written `[0-9]`: the two
/// matcher shapes in `datematcher`, the two capture shapes in
/// `daterange`, and the grammar document's own `match.value` patterns,
/// which `adjust_value_matchers` respells.
///
/// The key half and the `a = ٢٠٢٤-٠١-٠١` value are pinned in the shared
/// fixtures (`test/spec/errors.tsv`, `test/spec/basic-values.tsv`). This
/// test is what the fixtures cannot say: a `TomlTime` and a string
/// flatten to the SAME JSON, so only the KIND distinguishes a local time
/// from the text that merely looks like one.
#[test]
fn only_ascii_digits_make_a_date_or_time() {
    for src in [
        "a = \u{662}\u{660}\u{662}\u{664}-\u{660}\u{661}-\u{660}\u{661}",
        "a = \u{661}\u{662}:\u{663}\u{664}",
        "a = \u{661}\u{662}:\u{663}\u{664}:\u{663}\u{664}",
    ] {
        let value = field(src, "a");
        assert!(
            toml_time(&value).is_none(),
            "{src}: non-ASCII digits produced {value:?}"
        );
    }
}

// --- the leading BOM ----------------------------------------------------

/// A TOML document may start with a UTF-8 byte order mark and it is
/// ignored, but a BOM anywhere else is an error. Covered by
/// BurntSushi/toml-test `valid/utf8-bom-01` and `-02`, which only run
/// when that corpus is installed, hence this corpus-free guard. Mirrors
/// `TestLeadingBOM` in go/features_test.go and 'leading-bom' in
/// ts/test/toml.test.ts.
#[test]
fn leading_bom() {
    for src in ["\u{feff}a = 1", "\u{feff}# c\na = 1"] {
        assert_eq!(Value::Number(1.0), field(src, "a"), "{src:?}");
    }

    // Not at the start: still an error.
    assert!(
        parse("a = 1\n\u{feff}b = 2").is_err(),
        "a BOM after the first character should not be accepted"
    );

    // Also through a directly built instance, not just the `parse`
    // convenience: the matcher is on the engine, not the wrapper.
    assert_eq!(
        r#"{"a":1}"#,
        make().parse("\u{feff}a = 1").expect("a BOM").to_string()
    );
}

// --- error columns ------------------------------------------------------

/// Error COLUMNS after a non-ASCII character in a string.
///
/// This port brings its own string matcher, and a custom matcher owns the
/// arithmetic the engine's own matchers do for it. Here the scan hands
/// the engine a COUNT of Unicode scalar values and the engine does the
/// arithmetic, so there is no second implementation of it to drift; these
/// rows are what fails if that ever changes.
///
/// `go/strmatcher_col_test.go` and the TypeScript 'error columns count
/// characters, not bytes' test assert the same inputs. The astral rows
/// are the only ones where the answers differ, and that difference is the
/// recorded engine divergence: TypeScript counts UTF-16 units, and Go and
/// Rust count scalars. See `parser/DIVERGENCE.md`, "Column positions for
/// astral characters", and `../test/divergent.tsv`.
///
/// The Go file's two MALFORMED-UTF-8 rows have no counterpart here. The
/// engine parses a `&str`, which cannot hold a lone `0x80`, so the input
/// those rows are made of cannot be built at all.
#[test]
fn string_error_columns_count_scalars_not_bytes() {
    for (label, src, column, typescript) in [
        // Control: pure ASCII, where bytes and scalars coincide. Without
        // it, "columns are scalars" is also satisfied by never counting.
        ("ascii", "[a b]", 2, 2),
        // 2 and 3 bytes, 1 scalar, 1 UTF-16 unit: every port agrees.
        ("latin1", "[\"\u{e9}\" 1]", 5, 5),
        ("bmp", "[\"\u{20ac}\" 1]", 5, 5),
        // 4 bytes, 1 scalar, TWO UTF-16 units: the recorded divergence.
        ("astral", "[\"\u{1f600}\" 1]", 5, 6),
        ("mixed", "[\"ab\u{1f600}cd\" 1]", 9, 10),
    ] {
        let error = parse(src)
            .err()
            .unwrap_or_else(|| panic!("{label}: {src:?} parsed, expected a diagnostic"));
        assert_eq!(
            column, error.col,
            "{label}: {src:?} col = {}, want {column} (TypeScript says {typescript}). \
             A column ahead of the want by the character's extra BYTES means the scan is \
             counting bytes.",
            error.col
        );
    }
}

// --- the depth guard -----------------------------------------------------

/// The `n` segments of a dotted key or header, `a.a.….a`.
fn segments(n: usize) -> String {
    vec!["a"; n].join(".")
}

/// A document for a failure message, on one line, and its two ends only
/// when it is long: these documents differ at the end more than the start.
fn shown(src: &str) -> String {
    let line: Vec<char> = src.replace('\n', "\\n").chars().collect();
    if line.len() <= 60 {
        return line.into_iter().collect();
    }
    let head: String = line[..30].iter().collect();
    let tail: String = line[line.len() - 30..].iter().collect();
    format!("{head} ... {tail} ({} chars)", line.len())
}

fn assert_parses(parser: &Tabnas, src: &str) {
    if let Err(error) = parser.parse(src) {
        panic!("{}: {error}", shown(src));
    }
}

fn assert_refused(parser: &Tabnas, src: &str) {
    let error = parser
        .parse(src)
        .err()
        .unwrap_or_else(|| panic!("{}: must be refused", shown(src)));
    assert_eq!("cancel", error.code, "{}", shown(src));
}

/// Nesting past `DEPTH_LIMIT` levels is refused with the engine's `cancel`
/// code, whichever way a document nests (tabnas/toml#78): a dotted key, a
/// header, an array of tables, an inline table, an array, or a mix of
/// them. A level is a container the value sits in, the root table
/// included, so a dotted key of n segments nests n levels and a header of
/// n segments n + 1. TypeScript and Go have no limit, which
/// `../test/divergent.tsv` records with the two smallest documents refused
/// here; this test pins the boundaries, that width is not depth, that the
/// shared default parser is guarded too, and that lifting the guard lifts
/// every bound. jsonic's guard counted only its own containers, so an
/// inline table and an array were bounded already and a dotted key of
/// 10,000 segments parsed, at rule depth 10,002.
#[test]
fn nesting_is_bounded_by_the_depth_guard() {
    assert_eq!(127, DEPTH_LIMIT, "the limit is jsonic's");
    let parser = make();
    let parses = |src: &str| assert_parses(&parser, src);
    let refused = |src: &str| assert_refused(&parser, src);

    // A dotted key of n segments nests n levels: the root table, and a
    // table per segment but the last, which holds the value.
    parses(&format!("{} = 1", segments(127)));
    for n in [128, 129, 500, 10_000] {
        refused(&format!("{} = 1", segments(n)));
    }
    // A header of n segments nests n + 1: its tables sit in the root. The
    // header is refused at the segment past the limit, before its body
    // opens and before any value past the limit is built.
    parses(&format!("[{}]\nx = 1", segments(126)));
    refused(&format!("[{}]\nx = 1", segments(127)));
    refused(&format!("[{}]", segments(127)));
    refused(&format!("[{}]", segments(10_000)));
    // An array of tables holds its element in an array: one level more.
    parses(&format!("[[{}]]\nx = 1", segments(125)));
    refused(&format!("[[{}]]\nx = 1", segments(126)));
    // A dotted key counts from the table it is in.
    parses(&format!("[{}]\n{} = 1", segments(100), segments(27)));
    refused(&format!("[{}]\n{} = 1", segments(100), segments(28)));
    // An array and an inline table are a level each. In the root table
    // they are bounded where jsonic's guard, which this one replaces,
    // bounded them: 126 nest 127 levels with the root.
    let arrays = |n: usize| format!("x = {}{}", "[".repeat(n), "]".repeat(n));
    parses(&arrays(126));
    refused(&arrays(127));
    let tables = |n: usize| format!("x = {}1{}", "{a = ".repeat(n), "}".repeat(n));
    parses(&tables(126));
    refused(&tables(127));
    // A dotted key inside an inline table counts on from the key the table
    // is the value of: `a.a = {` is two levels a time.
    let inline = |n: usize| format!("{}a = 1{}", "a.a = {".repeat(n), "}".repeat(n));
    parses(&inline(63));
    refused(&inline(64));
    // Width is not depth: ten thousand dotted keys side by side, through
    // the dive's close loop.
    let wide: Vec<String> = (0..10_000).map(|i| format!("k{i}.a = {i}")).collect();
    parses(&wide.join("\n"));
    // The shared default parser is guarded too.
    assert_eq!(
        "cancel",
        parse(&format!("{} = 1", segments(10_000)))
            .unwrap_err()
            .code
    );
    // A caller that lifts the guard, by name, parses what it asked for,
    // and lifts jsonic's bound on arrays and inline tables with it: this
    // guard replaced that one under the same name, so none is left.
    let mut lifted = make();
    lifted.remove_parse_guard(DEPTH_GUARD);
    lifted
        .parse(&format!("{} = 1", segments(300)))
        .expect("the guard was lifted");
    lifted
        .parse(&arrays(300))
        .expect("no bound is left on arrays");
    lifted
        .parse(&tables(300))
        .expect("no bound is left on inline tables");
}

/// A dotted key in an inline table counts on from the keys outside it,
/// whichever way the parse reaches it. A key after a comma begins with a
/// new `pair`, but one after a newline or a space is taken by the dive's
/// close loop, in the frame of the key before it, and the guard once
/// counted such a key from the inline table, dropping the tables of every
/// key further out: two keys of 100 segments, one in the other's value,
/// parsed at 200 levels, and 26 levels of 99-segment keys built a value
/// 2,576 deep whose display ended the process. The test above never takes
/// the close loop inside an inline table.
#[test]
fn a_key_the_close_loop_takes_counts_from_the_keys_outside() {
    let parser = make();
    let parses = |src: &str| assert_parses(&parser, src);
    let refused = |src: &str| assert_refused(&parser, src);

    // A key of 100 segments and its inline table nest 101 levels, the
    // root included, and a key of n segments inside that table n - 1 more.
    let outer = segments(100);
    for between in ["\n", " ", ", "] {
        let inner = |n: usize| format!("{outer} = {{p.q = 1{between}{} = 1}}", segments(n));
        parses(&inner(27));
        refused(&inner(28));
        refused(&inner(100));
    }
    // Every level counts: four keys of 25 segments, each with its inline
    // table and on a new line of the one before, nest 101 levels, and a
    // key of n segments on a new line of the last n - 1 more.
    let nested = |n: usize| {
        format!(
            "{}{} = 1{}",
            format!("{} = {{p.q = 1\n", segments(25)).repeat(4),
            segments(n),
            "}".repeat(4)
        )
    };
    parses(&nested(27));
    refused(&nested(28));
    // So does a key whose value is an array: the root, `k.k`, the array
    // and the inline table in it nest four levels.
    let in_array = |n: usize| format!("k.k = [{{p.q = 1\n{} = 1}}]", segments(n));
    parses(&in_array(124));
    refused(&in_array(125));
}

/// Rows after a multi-line string (tabnas/toml#85). A multi-line string
/// trims the line feed right after its opening delimiter, and a
/// line-ending backslash trims the one right after it; both are still
/// lines of the source. The TypeScript and Go matchers own their row and
/// column arithmetic and consumed each of those line feeds without
/// counting a row, so every token and every diagnostic after such a string
/// was reported one row early there, per multi-line string before it. This
/// port hands the engine a count of characters and was right; the test
/// keeps it so. Both other ports assert the same rows.
#[test]
fn rows_after_a_multi_line_string_count_its_trimmed_line_feeds() {
    for (label, src, row, col) in [
        // Control: a multi-line string that trims nothing.
        ("no trim", "a = \"\"\"x\"\"\"\nb = ]", 2, 5),
        ("basic", "a = \"\"\"\nx\"\"\"\nb = ]", 3, 5),
        ("literal", "a = '''\nx'''\nb = ]", 3, 5),
        // One row early PER string before the error.
        (
            "two strings",
            "a = \"\"\"\nx\"\"\"\nb = \"\"\"\ny\"\"\"\nc = ]",
            5,
            5,
        ),
        // The line feed a line-ending backslash trims, then one it trims
        // after that.
        ("backslash", "a = \"\"\"\nx\\\n  y\"\"\"\nb = ]", 4, 5),
        (
            "backslash, blank line",
            "a = \"\"\"\nx\\\n\n  y\"\"\"\nb = ]",
            5,
            5,
        ),
    ] {
        let error = parse(src)
            .err()
            .unwrap_or_else(|| panic!("{label}: {src:?} parsed, expected a diagnostic"));
        assert_eq!(
            (error.row, error.col),
            (row, col),
            "{label}: {src:?} row:col"
        );
    }
}

/// The tokens themselves, as the lex trace a highlighter reads them: a
/// string token's point is the cursor AFTER the string (see
/// `strmatcher::end`), so it sits on the row the string ends on, and the
/// token after it starts on the next row. The empty string carries its two
/// quote characters as its source, as every other string token carries its
/// text. TypeScript and Go assert the same six traces.
#[test]
fn string_tokens_carry_their_source_and_end_on_the_row_they_end_on() {
    let trace = |src: &str| {
        let mut parser = make();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        parser.subscribe_lex(move |token, _rule, _context| {
            let name = token.name.as_str();
            if "#ST" == name || "#ID" == name {
                sink.lock().expect("the trace is whole").push(format!(
                    "{name}{:?}@{}:{}",
                    token.src.as_str(),
                    token.site.ri,
                    token.site.ci
                ));
            }
        });
        parser
            .parse(src)
            .unwrap_or_else(|error| panic!("{src:?}: {error}"));
        let traced = seen.lock().expect("the trace is whole").join(" ");
        traced
    };
    for (src, want) in [
        (
            "a = \"\"\"\nx\"\"\"\nb = 1",
            r#"#ID"a"@1:1 #ST"\"\"\"\nx\"\"\""@2:5 #ID"b"@3:1"#,
        ),
        (
            "a = '''\nx'''\nb = 1",
            r#"#ID"a"@1:1 #ST"'''\nx'''"@2:5 #ID"b"@3:1"#,
        ),
        (
            "a = \"\"\"\nx\\\n  y\"\"\"\nb = 1",
            r#"#ID"a"@1:1 #ST"\"\"\"\nx\\\n  y\"\"\""@3:7 #ID"b"@4:1"#,
        ),
        ("a = \"\"\nb = 1", r#"#ID"a"@1:1 #ST"\"\""@1:7 #ID"b"@2:1"#),
        ("a = ''\nb = 1", r#"#ID"a"@1:1 #ST"''"@1:7 #ID"b"@2:1"#),
        ("\"\" = 1", r#"#ST"\"\""@1:3"#),
    ] {
        assert_eq!(trace(src), want, "{src:?}");
    }
}

/// Columns after a multi-line string that ends with extra quotes. Up to
/// two quotes after the closing delimiter belong to the value, so
/// `"""x""""` is `x"`, and each is a column of the source too. The
/// TypeScript and Go matchers own their column arithmetic and consumed
/// those quotes without counting their columns, so everything after such
/// a string was placed one column early per extra quote there. This port
/// counts the token's characters and was right; the test keeps it so. The
/// register row `a = '''x''''''''''''''`, 1:18 in the other two ports
/// against 1:22 here, was that defect and not the engine's lookahead, and
/// it closed with it. Both other ports assert the same positions and
/// traces.
#[test]
fn columns_after_a_multi_line_string_count_its_extra_quotes() {
    for (label, src, row, col) in [
        // Control: no extra quote.
        ("none", "a = \"\"\"x\"\"\" ]", 1, 13),
        ("one", "a = \"\"\"x\"\"\"\" ]", 1, 14),
        ("two", "a = \"\"\"x\"\"\"\"\" ]", 1, 15),
        ("literal, one", "a = '''x'''' ]", 1, 14),
        ("literal, two", "a = '''x''''' ]", 1, 15),
    ] {
        let error = parse(src)
            .err()
            .unwrap_or_else(|| panic!("{label}: {src:?} parsed, expected a diagnostic"));
        assert_eq!(
            (error.row, error.col),
            (row, col),
            "{label}: {src:?} row:col"
        );
    }

    let trace = |src: &str| {
        let mut parser = make();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        parser.subscribe_lex(move |token, _rule, _context| {
            let name = token.name.as_str();
            if "#ST" == name || "#ID" == name {
                sink.lock().expect("the trace is whole").push(format!(
                    "{name}{:?}@{}:{}",
                    token.src.as_str(),
                    token.site.ri,
                    token.site.ci
                ));
            }
        });
        parser
            .parse(src)
            .unwrap_or_else(|error| panic!("{src:?}: {error}"));
        let traced = seen.lock().expect("the trace is whole").join(" ");
        traced
    };
    for (src, want) in [
        (
            "a = \"\"\"x\"\"\"\"\nb = 1",
            r#"#ID"a"@1:1 #ST"\"\"\"x\"\"\"\""@1:13 #ID"b"@2:1"#,
        ),
        (
            "a = '''x'''''\nb = 1",
            r#"#ID"a"@1:1 #ST"'''x'''''"@1:14 #ID"b"@2:1"#,
        ),
    ] {
        assert_eq!(trace(src), want, "{src:?}");
    }
}

// --- key names that are not special -------------------------------------

/// `__proto__` is an ordinary key. It is inert in Rust, where a map has
/// no prototype at all, and these rows say so rather than leaving the
/// reader to assume it: the canonical port had to ALLOCATE nodes without
/// a prototype to reach the same place, and a port that quietly dropped
/// the key would pass no test without them.
/// Mirrors ts/test/prototype-pollution.test.ts.
#[test]
fn proto_named_keys_are_ordinary() {
    for (src, want) in [
        (
            "[__proto__]\npwned = \"X\"\n",
            r#"{"__proto__":{"pwned":"X"}}"#,
        ),
        (
            "__proto__.pwned = \"X\"\n",
            r#"{"__proto__":{"pwned":"X"}}"#,
        ),
        (
            "[a.__proto__]\npwned = \"X\"\n",
            r#"{"a":{"__proto__":{"pwned":"X"}}}"#,
        ),
        (
            "[[__proto__]]\npwned = \"X\"\n",
            r#"{"__proto__":[{"pwned":"X"}]}"#,
        ),
        ("__proto__ = \"X\"\n", r#"{"__proto__":"X"}"#),
    ] {
        assert_eq!(
            want,
            parse(src)
                .unwrap_or_else(|e| panic!("{src:?}: {e}"))
                .to_string(),
            "{src:?}"
        );
    }
}

// --- key conflicts ------------------------------------------------------

/// TOML forbids redefining a key. These are DIAGNOSED rejections, with a
/// code and a position, rather than internal crashes, which is what the
/// `diagnosed` column of ../test/conformance.tsv counts. The position is
/// the key being redefined, as the Go and TypeScript ports report it; the
/// last eleven are shapes every port accepted until 2026-10-03: a key
/// given a second value, the same inside an inline table, a header written
/// twice, a header for a table a dotted key or an inline table had
/// defined, and a key a table already holds from an earlier header, which
/// the merge of the table's body used to replace.
#[test]
fn key_conflicts_are_diagnosed() {
    for (src, detail, at) in [
        (
            "a = 1\n[a.b]\nc = 2",
            "cannot define a, it already has the value 1",
            "2:2",
        ),
        (
            "[[a]]\nx = 1\n[a]\ny = 2",
            "cannot define a, it is already an array of tables",
            "3:2",
        ),
        (
            "a = 1\n[[a]]\nx = 1",
            "cannot define a, it already has the value 1",
            "2:3",
        ),
        (
            "a = 1\na = 2",
            "cannot define a, it already has the value 1",
            "2:1",
        ),
        (
            "a.b = 1\na.b = 2",
            "cannot define b, it already has the value 1",
            "2:3",
        ),
        (
            "a = {b = 1, b = 2}",
            "cannot define b, it already has the value 1",
            "1:13",
        ),
        ("[a]\n[a]", "cannot define a, it is already defined", "2:2"),
        (
            "a.b = 1\n[a]\nc = 2",
            "cannot define a, it is already defined",
            "2:2",
        ),
        (
            "[a]\nb.c = 1\n[a.b]\nd = 2",
            "cannot define b, it is already defined",
            "3:4",
        ),
        (
            "a = {}\n[a]",
            "cannot define a, it is already defined",
            "2:2",
        ),
        (
            "[a.b]\nc = 1\n[a]\nb = 2",
            r#"cannot define b, it already has the value {"c":1}"#,
            "4:1",
        ),
        (
            "[a.b.c]\nz = 1\n[a]\nb.c.t = 2",
            r#"cannot define b, it already has the value {"c":{"z":1}}"#,
            "4:1",
        ),
        (
            "[[a.b]]\n[a]\nb.y = 2",
            "cannot define b, it already has the value [{}]",
            "3:1",
        ),
        // A dotted key walking `x.a`, which `[x.a.b]`'s prefix created. The
        // marks are kept by path here, and `[x.a]` used to read the one
        // `[x.a.b]` left after the body had replaced the table at `x.a`.
        (
            "[x.a.b]\n[x]\na.c = 1\n[x.a]",
            r#"cannot define a, it already has the value {"b":{}}"#,
            "3:1",
        ),
    ] {
        let error = parse(src)
            .err()
            .unwrap_or_else(|| panic!("{src:?} parsed, expected a diagnostic"));
        assert_eq!("toml_key_conflict", error.code, "{src:?}");
        assert!(
            error.to_string().contains("cannot define"),
            "{src:?}: the code has no message template: {error}"
        );
        // The number renders as the engine's `to_json` writes it, `1.0`
        // for a value that is an integer in the source; the words around
        // it are the canonical port's.
        assert_eq!(detail, error.detail.replace("1.0", "1"), "{src:?}");
        assert_eq!(at, format!("{}:{}", error.row, error.col), "{src:?}");
    }

    // A header may define the table its own prefix created, once, and a new
    // sub-table under a table a dotted key made; a dotted key extends the
    // table it made; a later header's body adds a key its table does not
    // hold yet. None of these is a conflict. The last one is a body's own
    // `a`, whose path from the body's cell has the same trie node as the
    // implicit `a` at the top of the document: a dotted key walking it must
    // not clear that mark, or `[a]` is refused.
    for (src, want) in [
        (
            "[a.b]\nc = 1\n[a]\nd = 2",
            r#"{"a":{"b":{"c":1.0},"d":2.0}}"#,
        ),
        ("[[a.b]]\n[a]", r#"{"a":{"b":[{}]}}"#),
        (
            "[a]\nb.c = 1\n[a.b.d]\ne = 2",
            r#"{"a":{"b":{"c":1.0,"d":{"e":2.0}}}}"#,
        ),
        ("a.b = 1\na.c = 2", r#"{"a":{"b":1.0,"c":2.0}}"#),
        ("[a.b.c]\n[a]\nd = 1", r#"{"a":{"b":{"c":{}},"d":1.0}}"#),
        (
            "[a.b]\n[c]\na.x = 1\na.y = 2\n[a]",
            r#"{"a":{"b":{}},"c":{"a":{"x":1.0,"y":2.0}}}"#,
        ),
    ] {
        let value = parse(src).unwrap_or_else(|error| panic!("{src:?} must still parse: {error}"));
        assert_eq!(want, value.to_json().to_string(), "{src:?}");
    }
}

/// The template that sits UNDER each code in the canonical `error` and
/// `hint` tables, read out of `ts/src/toml.ts` by key rather than looked
/// for anywhere in the file.
///
/// Searching the whole source was the defect: with two codes and two
/// tables, swapping the two messages -- or giving one code the other's
/// hint -- left every string still present somewhere in `toml.ts`, so the
/// check passed while a reader got the wrong words for their diagnostic.
/// A template compared against "is this text in the file" is barely a
/// comparison at all.
///
/// The value may start on the key's line or the next, and is a
/// single-quoted string or a backtick template; neither of the four
/// contains its own delimiter.
fn canonical_template(source: &str, table: &str, code: &str) -> String {
    let opener = format!("{table}: {{");
    let block_at = source
        .find(&opener)
        .unwrap_or_else(|| panic!("ts/src/toml.ts opens a `{table}` block"));
    let block = &source[block_at + opener.len()..];
    let key = format!("{code}:");
    let key_at = block
        .find(&key)
        .unwrap_or_else(|| panic!("the `{table}` block declares no {code}"));
    let after = &block[key_at + key.len()..];
    let quote_at = after
        .find(['\'', '`'])
        .unwrap_or_else(|| panic!("the {table}.{code} value is not a literal"));
    let quote = after.as_bytes()[quote_at] as char;
    let body = &after[quote_at + 1..];
    let end = body
        .find(quote)
        .unwrap_or_else(|| panic!("the {table}.{code} literal is not closed"));
    body[..end].to_string()
}

/// The two toml-specific codes carry the CANONICAL message and hint, word
/// for word, so a document rejected by two ports is rejected in the same
/// words. `rs/AGENTS.md` and the templates in `lib.rs` both say so; this
/// measures it, by reading the installed options off a live instance and
/// the canonical text out of the source UNDER THE SAME CODE.
///
/// A reworded message on either side fails here, and so does a message
/// filed under the wrong code, which is the point: the code is the
/// contract across runtimes, and the wording is the contract with the
/// reader.
#[test]
fn the_error_templates_are_the_canonical_ones() {
    let path = repo_dir().join("ts").join("src").join("toml.ts");
    let canonical = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .replace("\r\n", "\n");
    let installed = make().config();

    for code in ["toml_key_conflict", "invalid_datetime"] {
        let message = installed
            .error
            .get(code)
            .unwrap_or_else(|| panic!("{code} has no message template"));
        let hint = installed
            .hint
            .get(code)
            .unwrap_or_else(|| panic!("{code} has no hint template"));

        assert_eq!(
            message.as_str(),
            canonical_template(&canonical, "error", code),
            "{code}: the message differs from the canonical one in {}",
            path.display()
        );
        assert_eq!(
            hint.as_str(),
            canonical_template(&canonical, "hint", code),
            "{code}: the hint differs from the canonical one in {}",
            path.display()
        );
    }

    // The extraction reads by key, so the two codes must come back with
    // DIFFERENT text; an extractor that returned the same string for both
    // would make every assertion above agree with itself.
    assert_ne!(
        canonical_template(&canonical, "error", "toml_key_conflict"),
        canonical_template(&canonical, "error", "invalid_datetime"),
        "the extraction is not reading by key"
    );
}

// --- the embedded grammar -----------------------------------------------

/// The grammar is authored once, in `../toml-grammar.jsonic`, and
/// embedded into all three runtimes by `ts/embed-grammar.js`. This is the
/// tripwire for an embed that was never re-run: it compares the constant
/// against the file on disk.
#[test]
fn the_embedded_grammar_is_the_file_on_disk() {
    let path = repo_dir().join("toml-grammar.jsonic");
    let on_disk = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    // The embed writes a newline after the opening `r#"`, so the constant
    // is the file with one leading newline. Nothing else is added: a Rust
    // raw string has no escapes.
    assert_eq!(
        format!("\n{on_disk}"),
        tabnas_toml::grammar_text(),
        "the embedded grammar and {} have drifted. Run `node ts/embed-grammar.js`; \
         never hand-edit between the BEGIN/END EMBEDDED markers.",
        path.display()
    );
}

/// Both `build-rs` targets regenerate the embedded grammar first.
///
/// The test above is the drift tripwire, and it is a TEST: it does not
/// run on a build. So a focused `make build-rs` after an edit to
/// `toml-grammar.jsonic` compiled the GRAMMAR_TEXT already in
/// `src/lib.rs`, reported success, and left the stale text in place,
/// where `build-go` has named `embed` as a prerequisite all along and
/// `build-ts` gets it from npm's own `build` script.
#[test]
fn the_rust_build_targets_embed_the_grammar_first() {
    for makefile in ["Makefile", "ts/Makefile"] {
        let path = repo_dir().join(makefile);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let rule = text
            .lines()
            .find(|line| line.starts_with("build-rs:"))
            .unwrap_or_else(|| panic!("{makefile} has no build-rs rule"));
        assert!(
            rule.split(':')
                .nth(1)
                .is_some_and(|after| after.split_whitespace().any(|want| "embed" == want)),
            "{makefile}: `{rule}` does not depend on embed, so a focused Rust build \
             compiles whatever grammar text src/lib.rs already holds"
        );
    }
}

// --- the setup instructions ---------------------------------------------

/// The sibling checkouts named in `[dependencies]` or `[dev-dependencies]`
/// of one manifest, as bare repository names: `path = "../../jsonic/rs"`
/// reads as `jsonic`.
fn siblings_of(manifest: &std::path::Path, section: &str) -> Vec<String> {
    let text = std::fs::read_to_string(manifest)
        .unwrap_or_else(|error| panic!("read {}: {error}", manifest.display()));
    let mut names = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == format!("[{section}]");
            continue;
        }
        let Some(rest) = line.split("path = \"").nth(1) else {
            continue;
        };
        let Some(path) = rest.split('"').next() else {
            continue;
        };
        if inside {
            if let Some(name) = path
                .strip_prefix("../../")
                .and_then(|tail| tail.strip_suffix("/rs"))
            {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Every sibling checkout a build needs, transitively, because a path
/// dependency of a path dependency is just as required as a direct one.
fn required_siblings(repo: &std::path::Path, section: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut queue = siblings_of(&repo.join("rs").join("Cargo.toml"), section);
    while let Some(name) = queue.pop() {
        if found.contains(&name) {
            continue;
        }
        let manifest = repo
            .parent()
            .expect("the repository has a parent directory")
            .join(&name)
            .join("rs")
            .join("Cargo.toml");
        // Only a checkout that is actually present can be walked further.
        // The gate in ci/rust/run.sh is what insists they all are.
        if manifest.is_file() {
            queue.extend(siblings_of(&manifest, "dependencies"));
        }
        found.push(name);
    }
    found.sort();
    found
}

/// Both Rust setup sections name every sibling checkout the build needs.
///
/// `tabnas-jsonic` takes the strict-JSON core as its OWN path dependency
/// on `../../json/rs`, and cargo reads every manifest in the graph before
/// it compiles anything, so a reader who cloned only the siblings the two
/// pages used to name got `failed to get tabnas-json as a dependency of
/// package tabnas-jsonic` and never reached an example. Deriving the list
/// from the manifests rather than restating it is what keeps the pages
/// right the next time a sibling gains one.
#[test]
fn the_setup_instructions_name_every_sibling_checkout() {
    let repo = repo_dir();
    let build = required_siblings(&repo, "dependencies");
    assert!(
        build.contains(&"json".to_string()),
        "expected the JSON core among the build siblings, got {build:?}"
    );

    for page in ["README.md", "rs/README.md"] {
        let path = repo.join(page);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for name in &build {
            let named = format!("github.com/tabnas/{name}");
            // `tabnas/jsonic` contains `tabnas/json`, so the mention has
            // to end where the name does.
            assert!(
                text.match_indices(&named).any(|(at, _)| {
                    text[at + named.len()..]
                        .chars()
                        .next()
                        .is_none_or(|next| !next.is_ascii_alphanumeric() && '-' != next)
                }),
                "{page} does not tell the reader to clone {named}, \
                 which the build needs: siblings {build:?}"
            );
        }
    }

    // The fixture runner is a development dependency, so only the crate's
    // own page promises it.
    let test_only = required_siblings(&repo, "dev-dependencies");
    let path = repo.join("rs").join("README.md");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    for name in &test_only {
        assert!(
            text.contains(&format!("github.com/tabnas/{name}")),
            "rs/README.md does not name the test-only sibling {name}"
        );
    }
}
