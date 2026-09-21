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
