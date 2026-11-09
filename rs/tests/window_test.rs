// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

//! A parse from a reader (`Tabnas::parse_reader`, admin ADR-30).
//!
//! `make()` builds the parser whole, as `tabnas_jsonic::make()` with the
//! plugin installed builds it, and declares `lex.window` on it, so that a
//! reader is read a window at a time. The plugin's own matchers, its
//! strings, its dates and times and the leading byte order mark, read the
//! source through the lexer's window and fill it where a scan reaches its
//! end. These tests hold it to that: `make()` declares the window and
//! parses as that composition does; every row of the shared fixtures
//! parses the same from a reader, in pieces of 1, 3 and 64 bytes, under
//! windows of a few characters and the engine's own, as from its text; and
//! tokens longer than the window, and a long document, parse the same too.
//! The toml-test corpus is held to the same in `toml_valid_test.rs`, beside
//! the walk that reads it.

mod common;

use common::{outcome, same_from_reader, short, spec_dir, Pieces};

/// Every fixture file, by name, in order.
fn fixture_files() -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(spec_dir())
        .expect("test/spec reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tsv"))
        .collect();
    files.sort();
    files
}

/// Every row of every fixture file: where it is, and its input.
fn fixture_rows() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for file in fixture_files() {
        let spec = tabnas_support::load_spec(
            spec_dir().join(&file),
            &tabnas_support::SpecOptions::default(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        for row in &spec.rows {
            rows.push((row.location(), row.unesc_named("input")));
        }
    }
    rows
}

/// The parser `make()` stands for: jsonic's base with the plugin
/// installed, which dmog composed before `make()` declared the window.
fn composed() -> tabnas::Tabnas {
    let mut parser = tabnas_jsonic::make();
    parser
        .use_plugin(tabnas_toml::plugin(), None)
        .expect("the plugin installs");
    parser
}

#[test]
fn make_declares_the_window() {
    assert!(tabnas_toml::make().options.lex.window);
    // The plugin alone declares nothing, in either form: a host may
    // install it beneath matchers of its own.
    assert!(!composed().options.lex.window);
    let mut parser = tabnas_jsonic::make();
    tabnas_toml::toml(&mut parser).expect("the grammar installs");
    assert!(!parser.options.lex.window);
}

#[test]
fn make_parses_as_the_composition_does() {
    let made = tabnas_toml::make();
    let composed = composed();
    let mut compared = 0;
    let mut failures = Vec::new();
    for (at, input) in fixture_rows() {
        let a = outcome(made.parse(&input));
        let b = outcome(composed.parse(&input));
        compared += 1;
        if a != b {
            failures.push(format!("{at}\n  make:     {a}\n  composed: {b}"));
        }
    }
    assert!(compared > 200, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_row_parses_the_same_from_a_reader() {
    let rows = fixture_rows();
    let mut compared = 0;
    let mut failures = Vec::new();
    for (index, (at, input)) in rows.iter().enumerate() {
        let (n, found) = same_from_reader(&tabnas_toml::make(), at, input);
        compared += n;
        failures.extend(found);
        if (index + 1) % 50 == 0 || index + 1 == rows.len() {
            println!(
                "window fixtures: {} of {} rows ({}%), {compared} reader parses",
                index + 1,
                rows.len(),
                (index + 1) * 100 / rows.len()
            );
        }
    }
    assert!(compared > 2400, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The engine's window reaches 64 KiB past the cursor before each token: a
/// token longer than that meets its edge under the engine's own sizes.
const LONG: usize = (64 << 10) + 2048;

#[test]
fn tokens_longer_than_the_window_parse_the_same_from_a_reader() {
    // Each kind of token the plugin cuts itself, and the engine's around
    // them, long, and cut short or refused past the edge.
    let docs = [
        format!("a = \"{}\\u00e9\\U0001F600\\x41\"\nb = 1", "s".repeat(LONG)),
        format!("a = \"{}\"", "é".repeat(LONG)),
        format!("a = '{}'", "l".repeat(LONG)),
        format!("a = \"\"\"\n{}\"\"\"\"\"", "m\\n\"x".repeat(LONG / 5)),
        format!(
            "a = \"\"\"{}\\\n{}z\"\"\"",
            "w".repeat(LONG),
            " \n\t".repeat(LONG / 3)
        ),
        format!("a = '''{}''''", "q'\n".repeat(LONG / 3)),
        format!("a = 1979-05-27T07:32:00.{}+07:00\nb = 2", "9".repeat(LONG)),
        format!("a = 07:32:00.{}\nb = 2", "5".repeat(LONG)),
        format!("a = 1979-05-27 07:32:00.{}Z", "1".repeat(LONG)),
        format!("k{}y = 1\n\"{}\" = 2", "e".repeat(LONG), "q".repeat(LONG)),
        format!("# {}\na = 1", "c".repeat(LONG)),
        format!("a = 1{}\n{}b = 2", " ".repeat(LONG), "\n".repeat(LONG)),
        format!("a = 1{}", "0".repeat(LONG)),
        format!("a = 1.{}e3", "5".repeat(LONG)),
        format!("a = [{}2]", "1, ".repeat(LONG / 3)),
        format!("a = \"{}", "u".repeat(LONG)),
        format!("a = \"\"\"{}", "u\n".repeat(LONG / 2)),
        format!("a = \"{}\u{1}\"", "p".repeat(LONG)),
        format!("a = \"{}\\x4\"", "x".repeat(LONG)),
        format!("a = \"{}\\u00", "x".repeat(LONG)),
        format!("a = 1979-05-27T07:32:00.{}+07:", "9".repeat(LONG)),
        format!("a = 2006-01-32T07:32:00.{}", "9".repeat(LONG)),
    ];
    let parser = tabnas_toml::make();
    let mut failures = Vec::new();
    for (n, doc) in docs.iter().enumerate() {
        let whole = outcome(parser.parse(doc));
        for size in [4096, (64 << 10) + 3] {
            let read = outcome(parser.parse_reader(Pieces::new(doc.as_bytes(), size)));
            if read != whole {
                failures.push(format!(
                    "document {n} in pieces of {size}: whole {}, reader {}",
                    short(&whole),
                    short(&read)
                ));
            }
        }
        println!(
            "window long tokens: {} of {} ({}%)",
            n + 1,
            docs.len(),
            (n + 1) * 100 / docs.len()
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A document of `records` arrays-of-tables elements, each with a
/// string, a literal string, dates and times, a float, an array holding an
/// inline table, a multi-line string with a line-ending backslash, and a
/// dotted key.
fn long_document(records: usize) -> String {
    let mut text = String::from("# records\ntitle = \"records\"\n");
    for i in 0..records {
        text.push_str(&format!(
            "\n[[record]]\nid = {i}\nname = \"ñ{i}\\u00e9\" # note {i}\n\
             path = 'C:\\dir\\{i}'\nwhen = 1979-05-27T07:32:{:02}.{i}-07:00\n\
             at = 07:32:00.5\nday = 2026-11-09\nscore = -1.5e3\n\
             tags = [\"a\", \"b\", {{ k = {i} }}]\n\
             note = \"\"\"\nline {i}\\\n  joined\"\"\"\ndeep.er.key = true\n",
            i % 60
        ));
    }
    text
}

#[test]
fn a_long_document_parses_the_same_from_a_reader() {
    // From a reader in pieces of 4 KiB, under the engine's window and
    // under one of 256 characters, which moves on thousands of times; with
    // an error at the end too, which the reader places where the whole
    // text does.
    let body = long_document(2500);
    let text = format!("{body}\n[end]\nok = true\n");
    assert!(text.len() > 300 << 10, "{}", text.len());
    let broken = format!("{body}\n[end]\nok = \"tail\u{1}\"\n");
    let parser = tabnas_toml::make();
    for (n, doc) in [&text, &broken].into_iter().enumerate() {
        let whole = outcome(parser.parse(doc));
        assert_eq!(whole.starts_with("ok "), 0 == n, "{}", short(&whole));
        let read = outcome(parser.parse_reader(Pieces::new(doc.as_bytes(), 4096)));
        assert_eq!(read, whole);
        let sizes = tabnas::WindowSizes {
            margin: 256,
            block: 256,
            chunk: 4096,
        };
        let read =
            outcome(parser.parse_reader_with_sizes(Pieces::new(doc.as_bytes(), 4096), sizes));
        assert_eq!(read, whole);
        println!("window long document: {} of 2 ({}%)", n + 1, (n + 1) * 50);
    }
}
