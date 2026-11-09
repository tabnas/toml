// Shared test helpers. Cargo compiles this module into EVERY integration
// test binary, so an item only one binary uses is dead code in the
// others; the allow keeps that from being a warning rather than hiding
// anything real.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tabnas_support::{find_spec_dir, Failure, Value};

/// The shared `test/spec` directory, found by walking up from the crate
/// rather than by counting `..` hops.
pub fn spec_dir() -> PathBuf {
    find_spec_dir(Some(Path::new(env!("CARGO_MANIFEST_DIR"))))
        .expect("a test/spec directory above rs/")
}

/// The repository root, one level above `test/`.
pub fn repo_dir() -> PathBuf {
    spec_dir()
        .parent()
        .and_then(Path::parent)
        .expect("test/spec sits two levels below the repository root")
        .to_path_buf()
}

/// An engine value as the fixture data model, through JSON.
pub fn to_value(value: &tabnas::Value) -> Value {
    Value::from(value.to_json())
}

/// A parse error as the runner's failure: the code the fixture pins, and
/// the rendered report for the failure message.
pub fn to_failure(error: tabnas::TabnasError) -> Failure {
    Failure::new(error.code.clone())
        .at(error.row, error.col)
        .with_message(error.to_string())
}

/// Parse one document with a FRESH parser, which is how the Go runner
/// runs every fixture row: no table state from one case can reach the
/// next.
pub fn parse_fresh(input: &str) -> Result<Value, Failure> {
    tabnas_toml::make()
        .parse(input)
        .map(|value| to_value(&value))
        .map_err(to_failure)
}

/// A reader that hands its bytes over at most `size` at a time, for a
/// parse from a reader (`Tabnas::parse_reader`, admin ADR-30).
pub struct Pieces<'a> {
    bytes: &'a [u8],
    at: usize,
    size: usize,
}

impl<'a> Pieces<'a> {
    pub fn new(bytes: &'a [u8], size: usize) -> Self {
        Pieces { bytes, at: 0, size }
    }
}

impl std::io::Read for Pieces<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.size.min(buf.len()).min(self.bytes.len() - self.at);
        buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// What a parse shows: its value, or its error's code, position, row,
/// column and token text. An error's excerpt is left out: a parse from a
/// reader shows a long line only as far back as the window reaches
/// (tabnas/parser `DIVERGENCE.md`), the one difference allowed.
pub fn outcome(parsed: Result<tabnas::Value, tabnas::TabnasError>) -> String {
    match parsed {
        Ok(value) => format!("ok {value:?}"),
        Err(e) => format!(
            "err {} at {} {}:{} {:?}",
            e.code, e.pos, e.row, e.col, e.src
        ),
    }
}

/// The pieces a reader hands its bytes over in.
pub const PIECES: [usize; 3] = [1, 3, 64];

/// The windows (margin and block, in characters) a reader is read under
/// besides the engine's own: a few characters, so that every matcher meets
/// the window's edge.
pub const WINDOWS: [(usize, usize); 3] = [(1, 1), (3, 2), (64, 64)];

/// A failure's text, cut to a length a report can carry.
pub fn short(text: &str) -> String {
    if text.chars().count() > 300 {
        format!("{}...", text.chars().take(300).collect::<String>())
    } else {
        text.to_string()
    }
}

/// `input` parsed by `parser` whole, and from a reader in each of
/// [`PIECES`], under the engine's window and under each of [`WINDOWS`]:
/// how many reader parses were compared, and where they differ.
pub fn same_from_reader(parser: &tabnas::Tabnas, at: &str, input: &str) -> (usize, Vec<String>) {
    let whole = outcome(parser.parse(input));
    let mut compared = 0;
    let mut failures = Vec::new();
    for size in PIECES {
        let mut check = |how: String, read: String| {
            compared += 1;
            if read != whole {
                failures.push(format!(
                    "{at}: {:?} from a reader in pieces of {size}{how}\n  whole:  {}\n  reader: {}",
                    short(input),
                    short(&whole),
                    short(&read)
                ));
            }
        };
        check(
            String::new(),
            outcome(parser.parse_reader(Pieces::new(input.as_bytes(), size))),
        );
        for (margin, block) in WINDOWS {
            let sizes = tabnas::WindowSizes {
                margin,
                block,
                chunk: size,
            };
            check(
                format!(", margin {margin}, block {block}"),
                outcome(parser.parse_reader_with_sizes(Pieces::new(input.as_bytes(), size), sizes)),
            );
        }
    }
    (compared, failures)
}
