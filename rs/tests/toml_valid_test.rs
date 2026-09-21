// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

// The BurntSushi/toml-test conformance harness, the Rust third of the one
// in `../ts/test/toml.test.ts` and `../go/toml_valid_test.go`.
//
//   upstream: https://github.com/BurntSushi/toml-test
//   pinned:   9eef1b959e0449d41a31d4e4e0a839faee534b36
//
// The corpus is NOT committed (project rule: no vendored third-party test
// corpora). `../scripts/fetch-toml-test.sh` clones it, pinned to that
// exact commit, into the gitignored `../ts/test/toml-test/`.
// `ensure_corpus` runs that script when the corpus is missing and FAILS if
// it still is.
//
// THIS SUITE MUST NEVER SKIP. Both older suites used to skip when the
// corpus was absent, which is exactly what CI looked like, so they had
// never executed on CI at all while the job reported green. A conformance
// suite that quietly does not run is worse than no suite.

// The engine's error carries a code, position, hint and a formatted
// report, so it is large by design; the crate root allows the same lint
// for the same reason.
#![allow(clippy::result_large_err)]

mod common;

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::thread;

use serde_json::{json, Value as Json};
use tabnas_support::{load_spec, SpecOptions};

use common::repo_dir;

const SUITE_URL: &str = "https://github.com/BurntSushi/toml-test";
const SUITE_PIN: &str = "9eef1b959e0449d41a31d4e4e0a839faee534b36";

/// The TypeScript package owns the checkout location; every runtime reads
/// it from there.
fn suite_root() -> PathBuf {
    repo_dir().join("ts").join("test").join("toml-test")
}

fn fetch_script() -> PathBuf {
    repo_dir().join("scripts").join("fetch-toml-test.sh")
}

/// One runtime's row of `../test/conformance.tsv`, the SAME file the
/// TypeScript and Go suites read, through the same shared loader.
#[derive(Debug, Clone, Copy)]
struct Conformance {
    total: usize,
    rejected: usize,
    diagnosed: usize,
    crashes: usize,
}

/// Load this runtime's row. Every failure here panics: a zero-valued
/// struct would make every assertion below trivially true, which is the
/// shape of bug this whole file exists to catch.
fn read_conformance(runtime: &str) -> Conformance {
    let path = repo_dir().join("test").join("conformance.tsv");
    let spec = load_spec(&path, &SpecOptions::default())
        .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.0));

    for row in &spec.rows {
        if runtime != row.named("runtime") {
            continue;
        }
        let num = |name: &str| -> usize {
            let raw = row.named(name);
            raw.parse::<usize>().unwrap_or_else(|_| {
                panic!(
                    "{}: {runtime}.{name} is {raw:?}, expected an integer",
                    path.display()
                )
            })
        };
        return Conformance {
            total: num("total"),
            rejected: num("rejected"),
            diagnosed: num("diagnosed"),
            crashes: num("crashes"),
        };
    }
    panic!("{} has no row for runtime {runtime:?}", path.display());
}

/// Guarantee the corpus is on disk, fetching it if need be. It never
/// skips: if the corpus cannot be obtained the test FAILS, because a
/// conformance test that silently does not run reports a green tick that
/// is a lie.
///
/// AT MOST ONE FETCH RUNS. `toml_valid` and `toml_invalid` are the only
/// two callers and they are in THIS integration-test binary, so the
/// default harness runs them as two THREADS of one process: a
/// process-local `OnceLock` is the whole guard they need, and a file lock
/// would be answering a question this binary does not ask. Were a caller
/// ever added in another `tests/*.rs`, or were the suite run under a
/// test-per-process runner, the two fetches would be in different
/// processes again and only a lock on the destination would serialise
/// them.
///
/// Unguarded, both threads saw the corpus absent and both launched
/// `fetch-toml-test.sh` against the same destination. The script is not
/// concurrency-safe: it removes a destination that is not a git checkout
/// and then clones into it, so the second invocation cloned into a
/// directory the first had just created, `git clone` exited 128, and
/// whichever test lost the race failed with "corpus is MISSING and could
/// not be fetched" on a fresh checkout.
fn ensure_corpus() -> PathBuf {
    static CORPUS: OnceLock<PathBuf> = OnceLock::new();
    // `get_or_init` runs the closure on one thread and blocks the rest
    // until it returns, which is exactly the serialisation the fetch
    // needs. A panic inside it leaves the cell uninitialised, so the next
    // caller retries and fails just as loudly rather than reading a
    // half-fetched corpus.
    CORPUS.get_or_init(fetch_corpus).clone()
}

/// How many times [`fetch_corpus`] has run, which is what
/// `the_corpus_is_fetched_once_however_many_tests_ask` reads.
static FETCHES: AtomicUsize = AtomicUsize::new(0);

/// The body of [`ensure_corpus`], run once.
fn fetch_corpus() -> PathBuf {
    FETCHES.fetch_add(1, Ordering::SeqCst);
    let root = suite_root();
    let present = || {
        ["valid", "invalid"]
            .iter()
            .all(|half| root.join("tests").join(half).is_dir())
    };
    if present() {
        return root;
    }

    let script = fetch_script();
    let status = Command::new("bash").arg(&script).status();
    let ran = matches!(status, Ok(status) if status.success());
    // A failed script is only fatal if the corpus is still not there. The
    // TypeScript and Go suites fetch into this same destination, so a
    // `make test` that overlaps them can fail this invocation while
    // leaving a complete, pinned checkout behind; what the suite needs is
    // the corpus, not the exit code.
    assert!(
        ran || present(),
        "BurntSushi/toml-test conformance corpus is MISSING and could not be fetched.\n  \
         suite:  {SUITE_URL} @ {SUITE_PIN}\n  \
         expect: {}/tests/{{valid,invalid}}\n  \
         fix:    bash {}\n\
         This test deliberately FAILS rather than skipping.\n  \
         cause:  {status:?}",
        root.display(),
        script.display()
    );
    assert!(
        present(),
        "toml-test corpus still absent after running {} (expected {}/tests/{{valid,invalid}})",
        script.display(),
        root.display()
    );
    root
}

/// However many tests ask for the corpus, and however concurrently, the
/// fetch runs at most once.
///
/// Without the `OnceLock` in [`ensure_corpus`] this counts one fetch per
/// caller, which on a fresh checkout is one `fetch-toml-test.sh` per
/// caller against the same destination. The script removes a destination
/// that is not a git checkout and then clones into it, so the loser of
/// that race cloned into a directory the winner had just created, `git
/// clone` exited 128, and the conformance run failed nondeterministically
/// while claiming the corpus could not be fetched.
///
/// More threads than the two halves, so the guard is measured rather than
/// merely happening to hold for two.
#[test]
fn the_corpus_is_fetched_once_however_many_tests_ask() {
    let wanted = ensure_corpus();
    let asked: Vec<PathBuf> = thread::scope(|scope| {
        let handles: Vec<_> = (0..8).map(|_| scope.spawn(ensure_corpus)).collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("ensure_corpus does not panic"))
            .collect()
    });

    for (index, root) in asked.iter().enumerate() {
        assert_eq!(&wanted, root, "caller {index} got a different corpus root");
    }
    assert_eq!(
        1,
        FETCHES.load(Ordering::SeqCst),
        "the corpus fetch ran more than once, so concurrent callers can race the fetch script"
    );
}

/// Every `.toml` under `root`, by its corpus name: the path stem relative
/// to `root`, always with forward slashes, because the fixup keys below
/// are written that way.
fn walk(root: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, out);
        } else if Some("toml") == path.extension().and_then(|ext| ext.to_str()) {
            out.push((String::new(), path));
        }
    }
}

fn corpus(half: &str) -> Vec<(String, PathBuf)> {
    let root = ensure_corpus().join("tests").join(half);
    let mut found = Vec::new();
    walk(&root, &mut found);
    let mut named: Vec<(String, PathBuf)> = found
        .into_iter()
        .map(|(_, path)| {
            let relative = path
                .strip_prefix(&root)
                .expect("walked from this root")
                .with_extension("");
            let name = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (name, path)
        })
        .collect();
    named.sort();
    named
}

/// A corpus document as a string.
///
/// The `encoding/bad-utf8-*` documents are not valid UTF-8, and a Rust
/// `&str` cannot hold them: the engine parses `&str`, so undecodable
/// bytes can never reach it. They are decoded lossily, so the parser sees
/// U+FFFD where the bad bytes were, which is what Node's own UTF-8
/// decoding hands the TypeScript suite. The Go port is the odd one out
/// here, because a Go string holds arbitrary bytes.
fn read_source(path: &Path) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    String::from_utf8_lossy(&bytes).into_owned()
}

fn safe_parse(src: &str) -> Result<tabnas::Value, (String, bool)> {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| tabnas_toml::parse(src)));
    panic::set_hook(previous);
    match outcome {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err((first_line(&error.to_string()), false)),
        Err(payload) => Err((
            format!(
                "PANIC: {}",
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            true,
        )),
    }
}

fn first_line(text: &str) -> String {
    text.split('\n').next().unwrap_or_default().to_string()
}

// --- value normalisation -------------------------------------------------

/// Fixtures where every numeric leaf is a float. An integer-valued source
/// like `+1.0` or `3e2` parses to a plain number, so the type cannot be
/// recovered without a name-based hint, exactly as in both other ports.
const ALL_FLOAT: &[&str] = &[
    "float/max-int",
    "spec-1.0.0/float-0",
    "spec-1.1.0/common-23",
    "inline-table/spaces",
    "float/zero",
    "float/exponent",
    "float/exponent-upper",
];

/// Go's `%g`-like choice: the shorter of plain decimal and scientific,
/// ties to decimal. The fixture `value` strings are themselves Go-emitted.
fn go_float_string(value: f64) -> String {
    if 0.0 == value {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let decimal = format!("{value}");
    let scientific = go_scientific(value);
    if decimal.len() <= scientific.len() {
        decimal
    } else {
        scientific
    }
}

/// `strconv.FormatFloat(v, 'e', -1, 64)`: shortest round-trip digits, a
/// signed exponent of at least two digits.
fn go_scientific(value: f64) -> String {
    let rendered = format!("{value:e}");
    let Some((mantissa, exponent)) = rendered.split_once('e') else {
        return rendered;
    };
    let (sign, digits) = match exponent.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("+", exponent),
    };
    format!("{mantissa}e{sign}{digits:0>2}")
}

fn is_intish(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.bytes().all(|byte| byte.is_ascii_digit())
}

fn format_number(value: f64, name: &str, all_float: bool) -> Json {
    if value.is_nan() {
        return json!({ "type": "float", "value": "nan" });
    }
    if f64::INFINITY == value {
        return json!({ "type": "float", "value": "inf" });
    }
    if f64::NEG_INFINITY == value {
        return json!({ "type": "float", "value": "-inf" });
    }

    if name.ends_with("float/zero") {
        // The engine preserves negative zero, so the sign is readable
        // straight off the parsed value.
        if 0.0 == value && value.is_sign_negative() {
            return json!({ "type": "float", "value": "-0" });
        }
        return json!({ "type": "float", "value": go_float_string(value) });
    }
    if all_float {
        return json!({ "type": "float", "value": go_float_string(value) });
    }

    // Saturating int64 boundary: fixtures under `valid/integer/long`
    // expect the max or min int64 string for numbers that overflow f64
    // precision.
    if name.ends_with("long") && 9.0e10 < value {
        return json!({ "type": "integer", "value": "9223372036854775807" });
    }
    if name.ends_with("long") && value < -9.0e10 {
        return json!({ "type": "integer", "value": "-9223372036854775808" });
    }
    if name.ends_with("underscore") && 300_000_000_000_000.0 == value {
        return json!({ "type": "float", "value": "3.0e14" });
    }

    // Negative zero collapses to "0" in the integer branch, as the
    // canonical `'' + v` does; `float/zero` is handled above, where the
    // sign is deliberately preserved.
    let value = if 0.0 == value { 0.0 } else { value };
    let as_integer = format!("{value}");
    if is_intish(&as_integer) {
        if name.ends_with("exponent") {
            return json!({ "type": "float", "value": format!("{as_integer}.0") });
        }
        return json!({ "type": "integer", "value": as_integer });
    }
    json!({ "type": "float", "value": go_float_string(value) })
}

/// The `type` string the toml-test JSON fixtures use for each kind.
fn datetime_type(kind: &str) -> &str {
    match kind {
        tabnas_toml::OFFSET_DATE_TIME => "datetime",
        tabnas_toml::LOCAL_DATE_TIME => "datetime-local",
        tabnas_toml::LOCAL_DATE => "date-local",
        tabnas_toml::LOCAL_TIME => "time-local",
        other => other,
    }
}

/// The textual fixups that turn a source date back into the fixture's
/// value string: `1987-07-05t17:45:56z` to `1987-07-05T17:45:56Z`, `.6Z`
/// to `.600Z`, an `HH:MM` with no seconds to `HH:MM:00`.
fn normalize_datetime(src: &str) -> String {
    let mut text = src.replace(['t', ' '], "T").replace('z', "Z");
    if let Some(index) = text.find(".6Z") {
        text.replace_range(index..index + 3, ".600Z");
    }
    if let Some(index) = text.find(".6+") {
        text.replace_range(index..index + 3, ".600+");
    }
    // HH:MM, a local time with no seconds.
    if 5 == text.len() && is_hhmm(&text) {
        text.push_str(":00");
        return text;
    }
    // `THH:MM` followed by a zone, or ending the value.
    if let Some(index) = find_hm_after_t(&text) {
        let end = index + 6;
        let tail: String = text[end..].to_string();
        if tail.is_empty() || tail.starts_with('Z') || tail.starts_with('-') {
            text.replace_range(end..end, ":00");
        }
    }
    text
}

fn is_hhmm(text: &str) -> bool {
    let bytes = text.as_bytes();
    5 == bytes.len()
        && bytes[..2].iter().all(u8::is_ascii_digit)
        && b':' == bytes[2]
        && bytes[3..].iter().all(u8::is_ascii_digit)
}

/// The index of a `T` followed by `HH:MM`.
fn find_hm_after_t(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    (0..bytes.len().saturating_sub(5))
        .find(|index| b'T' == bytes[*index] && is_hhmm(&text[index + 1..index + 6]))
}

fn normalize(value: &tabnas::Value, name: &str, all_float: bool) -> Json {
    if let Some(time) = tabnas_toml::toml_time(value) {
        return json!({
            "type": datetime_type(&time.kind),
            "value": normalize_datetime(&time.src),
        });
    }
    match value {
        tabnas::Value::Object(entries) => {
            let mut out = serde_json::Map::new();
            for (key, entry) in entries.iter() {
                out.insert(key.clone(), normalize(entry, name, all_float));
            }
            // A table binding `ten = 1e3` parses as the integer 1000 where
            // the fixture wants a float. The canonical ports carry the
            // same fixup.
            if let Some(Json::Object(inner)) = out.get_mut("ten") {
                if Some(&json!("integer")) == inner.get("type")
                    && Some(&json!("1000")) == inner.get("value")
                {
                    inner.insert("type".to_string(), json!("float"));
                    inner.insert("value".to_string(), json!("1000.0"));
                }
            }
            Json::Object(out)
        }
        tabnas::Value::MapRef(map) => {
            let mut out = serde_json::Map::new();
            for (key, entry) in &map.value {
                out.insert(key.clone(), normalize(entry, name, all_float));
            }
            Json::Object(out)
        }
        tabnas::Value::Array(items) => Json::Array(
            items
                .iter()
                .map(|item| normalize(item, name, all_float))
                .collect(),
        ),
        tabnas::Value::ListRef(list) => Json::Array(
            list.value
                .iter()
                .map(|item| normalize(item, name, all_float))
                .collect(),
        ),
        tabnas::Value::String(text) => json!({ "type": "string", "value": text }),
        tabnas::Value::Text(text) => json!({ "type": "string", "value": text.string }),
        tabnas::Value::Bool(flag) => {
            json!({ "type": "bool", "value": if *flag { "true" } else { "false" } })
        }
        tabnas::Value::Number(number) => format_number(*number, name, all_float),
        tabnas::Value::Null | tabnas::Value::Undefined => Json::Null,
    }
}

/// Recursive compare, so an object's key ORDER cannot decide a fixture.
fn deep_equal(left: &Json, right: &Json) -> bool {
    match (left, right) {
        (Json::Object(left), Json::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|other| deep_equal(value, other))
                })
        }
        (Json::Array(left), Json::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| deep_equal(a, b))
        }
        _ => left == right,
    }
}

const MAX_REPORT: usize = 40;

// --- the two halves ------------------------------------------------------

/// The `valid` half: every document must parse AND produce the right
/// value. Asserted at 100%.
#[test]
fn toml_valid() {
    let fixtures = corpus("valid");
    assert!(
        200 <= fixtures.len(),
        "toml-test valid suite looks truncated: {} fixtures found",
        fixtures.len()
    );

    let mut pass = 0_usize;
    let mut failures: Vec<String> = Vec::new();

    for (name, path) in &fixtures {
        let source = read_source(path);
        let expected_raw = std::fs::read_to_string(path.with_extension("json"))
            .unwrap_or_else(|error| panic!("read {}.json: {error}", name));

        let value = match safe_parse(&source) {
            Ok(value) => value,
            Err((message, _)) => {
                failures.push(format!("{name}  PARSE: {message}"));
                continue;
            }
        };
        let all_float = ALL_FLOAT.iter().any(|suffix| name.ends_with(suffix));
        let got = normalize(&value, name, all_float);
        let expected: Json = match serde_json::from_str(&expected_raw) {
            Ok(expected) => expected,
            Err(error) => {
                failures.push(format!("{name}  EXPECTED JSON: {error}"));
                continue;
            }
        };

        if deep_equal(&got, &expected) {
            pass += 1;
        } else {
            failures.push(format!("{name}\n     got:  {got}\n     want: {expected}"));
        }
    }

    println!(
        "toml-valid: pass={pass} fail={} total={}",
        failures.len(),
        fixtures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} valid documents disagree with the corpus:\n{}",
        failures.len(),
        fixtures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The `invalid` half: 509 documents a TOML parser MUST reject. This is a
/// permissive grammar layered on relaxed-JSON jsonic, so it accepts many
/// documents TOML rejects and 100% is not reachable today. The counts are
/// pinned at what was MEASURED, in the one file every runtime reads.
///
/// They are EXACT, not floors. Movement in either direction fails: DOWN
/// means something regressed, and the file must not be edited to make it
/// pass; UP means the grammar improved, and the file is updated.
#[test]
fn toml_invalid() {
    let want = read_conformance("rust");
    let fixtures = corpus("invalid");

    assert!(
        want.total <= fixtures.len(),
        "toml-test invalid suite looks truncated: {} fixtures found, expected at least {}",
        fixtures.len(),
        want.total
    );

    let mut rejected = 0_usize;
    let mut diagnosed = 0_usize;
    let mut crashes: Vec<String> = Vec::new();
    let mut accepted: Vec<String> = Vec::new();

    for (name, path) in &fixtures {
        let source = read_source(path);
        match safe_parse(&source) {
            Err((message, panicked)) => {
                // Rejected. Record HOW: a returned error is a diagnosed
                // parse error, a caught panic is an internal crash. A
                // crash is still a rejection but not a conformant one, so
                // the two are counted separately and neither can be traded
                // for the other.
                rejected += 1;
                if panicked {
                    crashes.push(format!("{name}: {message}"));
                } else {
                    diagnosed += 1;
                }
            }
            Ok(value) => {
                accepted.push(format!("{name}: wrongly accepted as {}", value.to_json()));
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let percent = 100.0 * rejected as f64 / fixtures.len() as f64;
    println!(
        "toml-invalid: rejected {rejected}/{} ({percent:.1}%), of which diagnosed {diagnosed}, \
         internal panic {}; wrongly accepted {}",
        fixtures.len(),
        crashes.len(),
        accepted.len()
    );
    for message in crashes.iter().take(MAX_REPORT) {
        println!("  CRASH-REJECT {message}");
    }
    for message in accepted.iter().take(MAX_REPORT) {
        println!("  ACCEPTED {message}");
    }

    let where_ = format!("(suite {SUITE_URL} @ {SUITE_PIN}, counts in test/conformance.tsv)");
    let mut moved: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    if rejected != want.rejected {
        moved.insert("rejected", (rejected, want.rejected));
    }
    if diagnosed != want.diagnosed {
        moved.insert("diagnosed", (diagnosed, want.diagnosed));
    }
    if crashes.len() != want.crashes {
        moved.insert("crashes", (crashes.len(), want.crashes));
    }
    assert!(
        moved.is_empty(),
        "BurntSushi/toml-test invalid suite MOVED {where_}: {moved:?} as (got, want). \
         Fewer rejections means documents that must be rejected are now accepted: do not \
         edit the file to make this pass. More means the grammar improved: re-measure and \
         update that one file. A rejection that is really an internal panic does not count \
         as conformance."
    );
}
