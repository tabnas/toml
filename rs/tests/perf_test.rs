// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

// `parse` must reuse one instance rather than rebuilding the grammar per
// call. Building the grammar dominates a parse here: the text is parsed
// by a whole second engine and then installed. Mirrors
// `TestParseReusesInstance` in go/perf_test.go and ts/test/perf.test.ts.
//
// The check is machine-INDEPENDENT. It compares `parse` against instance
// reuse on the SAME machine in the SAME run, so a slow or loaded CI box
// cannot make it flaky: both sides scale together. There is deliberately
// NO wall-clock budget.

use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use tabnas::Value;

const SRC: &str = "a = 1\nb = 2\nc = 3";
const RUNS: usize = 2000;
const WARMUP: usize = 100;

#[test]
fn parse_reuses_its_instance() {
    // Warm both paths so the comparison is steady-state.
    for _ in 0..WARMUP {
        let _ = tabnas_toml::parse(SRC);
    }
    let parser = tabnas_toml::make();
    for _ in 0..WARMUP {
        let _ = parser.parse(SRC);
    }

    let start = Instant::now();
    for _ in 0..RUNS {
        tabnas_toml::parse(SRC).expect("the convenience path parses");
    }
    let convenience = start.elapsed();

    let start = Instant::now();
    for _ in 0..RUNS {
        parser.parse(SRC).expect("the reused instance parses");
    }
    let reuse = start.elapsed();

    // A cached `parse` is about the same as instance reuse; 4x is slack
    // for scheduling noise. A rebuild-per-call `parse` is many times
    // slower here, so this catches the regression without depending on
    // absolute speed.
    assert!(
        convenience <= 4 * reuse,
        "parse() appears to rebuild the grammar on every call: {RUNS} calls took {convenience:?} \
         against {reuse:?} reusing one instance (ratio {:.1}x, limit 4x). Cache a lazy default \
         instance (see `parse` and its OnceLock).",
        convenience.as_secs_f64() / reuse.as_secs_f64().max(f64::MIN_POSITIVE)
    );
    println!(
        "parse()={convenience:?}  reuse={reuse:?}  ratio={:.2}x",
        convenience.as_secs_f64() / reuse.as_secs_f64().max(f64::MIN_POSITIVE)
    );
}

/// TOML of `n` array tables, each with strings in it.
fn tables(n: usize) -> String {
    use std::fmt::Write;
    let mut src = String::new();
    for i in 0..n {
        write!(
            src,
            "[[item]]\nid = {i}\nname = \"item {i}\"\ntags = [\"a\", \"b\"]\n\n"
        )
        .expect("a String takes any write");
    }
    src
}

// A parse takes time in proportion to the length of the document. The
// string matcher used to copy the whole of the rest of the source at every
// token, before it had even looked for a quote, so parse time grew with the
// SQUARE of the length: 4,000 tables took 23 seconds rather than 0.4.
// Mirrors `TestParseIsLinear` in go/perf_test.go and ts/test/perf.test.ts.
//
// Machine-independent like the test above: it compares a document with
// four times as much in it, in the same run. Linear time makes that about
// 4x; quadratic made it 16x. The limit, 8x, sits between.
#[test]
fn parse_time_is_linear_in_document_length() {
    const SMALL: usize = 250;
    let parser = tabnas_toml::make();
    let time = |src: &str| {
        (0..3)
            .map(|_| {
                let start = Instant::now();
                parser.parse(src).expect("the tables parse");
                start.elapsed()
            })
            .min()
            .expect("three runs")
    };
    let small = time(&tables(SMALL));
    let large = time(&tables(4 * SMALL));
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::MIN_POSITIVE);
    assert!(
        ratio <= 8.0,
        "parse time grows faster than document length: {SMALL} tables took {small:?} and \
         {} took {large:?} (ratio {ratio:.1}x; linear is about 4x, quadratic 16x). Something \
         per token is reading the rest of the source.",
        4 * SMALL
    );
    println!(
        "{SMALL} tables={small:?}  {} tables={large:?}  ratio={ratio:.2}x",
        4 * SMALL
    );
}

// --- dotted table headers ------------------------------------------------

/// A thread stack big enough to DROP a value nested as deep as the headers
/// below make it. A parse does not recurse once per level; the derived
/// drop of a `Value` does, and in a debug build it needs well over a
/// kilobyte a level. That is the caller's stack, not the parse's.
const BIG_STACK: usize = 256 * 1024 * 1024;

fn on_big_stack<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> thread::JoinHandle<T> {
    thread::Builder::new()
        .stack_size(BIG_STACK)
        .spawn(work)
        .expect("spawn a big-stack thread")
}

/// The `n` dotted segments of a header, `key(0).key(1).….key(n - 1)`.
fn dotted(n: usize, key: impl Fn(usize) -> String) -> String {
    (0..n).map(key).collect::<Vec<_>>().join(".")
}

/// One parse of `src`, timed. The value is dropped once the clock stops.
fn time_parse(src: &str) -> Duration {
    let start = Instant::now();
    let value = tabnas_toml::parse(src).expect("the header parses");
    let took = start.elapsed();
    drop(value);
    took
}

// A dotted table header takes time in proportion to its number of
// segments (tabnas/toml#81). Every segment used to walk the tree from its
// root, rebuild the whole path several times over and leave one more copy
// of it on a rule the engine keeps, so a header of 2,000 segments took a
// second and one of 10,000 two minutes in a release build, where the other
// two ports take milliseconds. Mirrors `TestDottedHeaderIsLinear` in
// go/perf_test.go and 'a dotted header takes time in proportion to its
// length' in ts/test/perf.test.ts.
//
// Machine-independent like the test above: it compares a header four
// times as long, in the same run. Linear time makes that about 4x; the old
// code made it 30x and more. The limit, 8x, sits between. A loaded runner
// can slow either side, so the short header is timed three times and the
// fastest kept, and the comparison gets three attempts, each against a
// fresh measurement of the short one. The long header is parsed on a
// thread of its own against a deadline of 8x the short one, because the
// old code takes many minutes over it in a debug build: a regression fails
// here within two minutes rather than holding the job until it is killed.
#[test]
fn a_dotted_header_takes_time_in_proportion_to_its_length() {
    const SHORT: usize = 2_000;
    const LIMIT: u32 = 8;
    let header = |n: usize| format!("[{}]\nx = 1\n", dotted(n, |_| "a".to_string()));
    let short = header(SHORT);
    let long = Arc::new(header(4 * SHORT));
    let outcome = on_big_stack(move || {
        // The first parse builds the shared parser.
        time_parse(&short);
        let mut shorts = Vec::new();
        for _ in 0..3 {
            let few = (0..3)
                .map(|_| time_parse(&short))
                .min()
                .expect("three runs");
            shorts.push(few);
            let (send, receive) = mpsc::channel();
            let src = Arc::clone(&long);
            on_big_stack(move || send.send(time_parse(&src)));
            if let Ok(many) = receive.recv_timeout(few * LIMIT) {
                return Ok((few, many));
            }
        }
        Err(shorts)
    })
    .join()
    .expect("the measuring thread finished");
    match outcome {
        Ok((few, many)) => println!(
            "{SHORT} segments={few:?}  {} segments={many:?}  ratio={:.2}x",
            4 * SHORT,
            many.as_secs_f64() / few.as_secs_f64().max(f64::MIN_POSITIVE)
        ),
        Err(shorts) => panic!(
            "a dotted header's parse time grows faster than its length: {SHORT} segments took \
             {shorts:?} in three attempts, and {} segments took more than {LIMIT}x that every \
             time (linear is about 4x, quadratic 16x). Something per segment is walking the \
             path from the root, copying it, or keeping a copy of it.",
            4 * SHORT
        ),
    }
}

// A header thousands of segments long builds exactly the tables it names,
// and later headers walk back down through them: one adds a table beside
// the first one's key, two arrays of tables append to the same array, and
// one that treats a key holding a value as a table is refused, with the
// same diagnosis as a short header gets. Mirrors
// `TestLongDottedHeaderValue` in go/perf_test.go, which has no diagnosis
// to check, and 'a long dotted header builds every table it names' in
// ts/test/perf.test.ts.
#[test]
fn a_long_dotted_header_builds_every_table_it_names() {
    on_big_stack(|| {
        const DEPTH: usize = 5_000;
        let path = dotted(DEPTH, |i| format!("k{i}"));
        let src = format!(
            "[{path}]\nx = 1\n[{path}.y]\nz = 2\n[[{path}.list]]\nn = 1\n[[{path}.list]]\nn = 2\n"
        );
        let value = tabnas_toml::parse(&src).expect("the long headers parse");

        // Walked with a loop: a value 5,000 deep is not compared with the
        // call stack.
        let mut table = &value;
        for depth in 0..DEPTH {
            let Value::Object(map) = table else {
                panic!("level {depth} is not a table");
            };
            let key = format!("k{depth}");
            assert_eq!(
                map.keys().collect::<Vec<_>>(),
                [&key],
                "level {depth} holds {key} and nothing else"
            );
            table = &map[&key];
        }
        assert_eq!(
            table.to_string(),
            r#"{"x":1,"y":{"z":2},"list":[{"n":1},{"n":2}]}"#
        );

        let error = tabnas_toml::parse(&format!("{src}[{path}.x.q]\n"))
            .expect_err("x holds a value, so it is not a table to add to");
        assert_eq!("toml_key_conflict", error.code);
        assert!(
            error
                .to_string()
                .contains("cannot define x, it already has the value 1"),
            "{error}"
        );
    })
    .join()
    .expect("the long headers were checked to the end");
}
