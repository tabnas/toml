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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use tabnas::{Tabnas, Value};
use tabnas_toml::{make, DEPTH_GUARD, DEPTH_LIMIT};

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

/// The `n` dotted segments of a header or key, `key(0).key(1).….key(n - 1)`.
fn dotted(n: usize, key: impl Fn(usize) -> String) -> String {
    (0..n).map(key).collect::<Vec<_>>().join(".")
}

/// A parser with the depth guard lifted. The guard refuses nesting past
/// `DEPTH_LIMIT` levels, and the headers and keys below are far past it on
/// purpose: they measure the algorithm, which has to be linear whatever the
/// limit is, and run on a stack that can drop what they build.
fn unguarded() -> Arc<Tabnas> {
    let mut parser = make();
    parser.remove_parse_guard(DEPTH_GUARD);
    Arc::new(parser)
}

/// One parse of `src`, timed. The value is dropped once the clock stops.
fn time_parse(parser: &Tabnas, src: &str) -> Duration {
    let start = Instant::now();
    let value = parser.parse(src).expect("the document parses");
    let took = start.elapsed();
    drop(value);
    took
}

/// The time a document four times as long takes against a short one,
/// measured as `a_dotted_header_takes_time_in_proportion_to_its_length`
/// explains: the short one timed three times and the fastest kept, the
/// comparison tried three times, the long one parsed on a thread of its
/// own against a deadline of `limit` times the short one.
fn ratio_of_four_times(
    parser: Arc<Tabnas>,
    short: String,
    long: String,
    limit: u32,
) -> Result<(Duration, Duration), Vec<Duration>> {
    let long = Arc::new(long);
    time_parse(&parser, &short);
    let mut shorts = Vec::new();
    for _ in 0..3 {
        let few = (0..3)
            .map(|_| time_parse(&parser, &short))
            .min()
            .expect("three runs");
        shorts.push(few);
        let (send, receive) = mpsc::channel();
        let src = Arc::clone(&long);
        let timed = Arc::clone(&parser);
        on_big_stack(move || send.send(time_parse(&timed, &src)));
        if let Ok(many) = receive.recv_timeout(few * limit) {
            return Ok((few, many));
        }
    }
    Err(shorts)
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
    let long = header(4 * SHORT);
    let outcome = on_big_stack(move || ratio_of_four_times(unguarded(), short, long, LIMIT))
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
        let parser = unguarded();
        let value = parser.parse(&src).expect("the long headers parse");

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

        let error = parser
            .parse(&format!("{src}[{path}.x.q]\n"))
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

// --- dotted keys -----------------------------------------------------------

/// The engine's rule depth `d` the deepest rule of a parse reaches, read
/// through a rule subscriber, and how the parse ended. With `lift` the
/// depth guard is removed first, so the whole document is parsed.
fn deepest_rule(src: &str, lift: bool) -> (usize, Result<(), String>) {
    let mut parser = make();
    if lift {
        parser.remove_parse_guard(DEPTH_GUARD);
    }
    let seen = Arc::new(AtomicUsize::new(0));
    let sink = Arc::clone(&seen);
    parser.subscribe_rules(move |rule, _context| {
        sink.fetch_max(rule.d, Ordering::Relaxed);
    });
    let outcome = parser.parse(src).map(|_| ()).map_err(|error| error.code);
    (seen.load(Ordering::Relaxed), outcome)
}

// A dotted key is a replace loop (tabnas/toml#78): each segment ending in
// a dot re-enters `dive` in the same frame, so the engine's rule depth `d`
// stays what one segment needs however long the key. It used to push a
// dive per segment, so `d` was the segment count plus two, 10,002 for ten
// thousand segments, past the 3,000 open rules aless allows a parse. A
// header, which was always a loop, is the control. Under the guard a long
// key is refused at the limit, at the same depth; with the guard lifted
// the whole key is parsed at it. Mirrors TestDottedKeyRuleDepthIsConstant
// in go/perf_test.go and 'a dotted key keeps rule depth constant' in
// ts/test/perf.test.ts, where there is no guard to lift.
#[test]
fn a_dotted_key_keeps_rule_depth_constant() {
    on_big_stack(|| {
        let key = |n: usize| format!("{} = 1", dotted(n, |_| "a".to_string()));
        let header = |n: usize| format!("[{}]\nx = 1", dotted(n, |_| "a".to_string()));

        let (two, outcome) = deepest_rule(&key(2), true);
        outcome.expect("a two-segment key parses");
        let (many, outcome) = deepest_rule(&key(10_000), true);
        outcome.expect("a ten-thousand-segment key parses with the guard lifted");
        assert_eq!(
            two, many,
            "ten thousand segments reach rule depth {many}, where two reach {two}: the dive \
             is a push chain again"
        );
        let (two_h, _) = deepest_rule(&header(2), true);
        let (many_h, outcome) = deepest_rule(&header(10_000), true);
        outcome.expect("a ten-thousand-segment header parses with the guard lifted");
        assert_eq!(two_h, many_h, "a header's rule depth grew with its length");

        let (seen, outcome) = deepest_rule(&key(10_000), false);
        assert_eq!(
            Err("cancel".to_string()),
            outcome,
            "the guard refuses the long key"
        );
        assert!(
            seen <= two,
            "the refused key reached rule depth {seen}, deeper than a short key's {two}"
        );
    })
    .join()
    .expect("the depths were measured");
}

// A dotted key takes time in proportion to its number of segments
// (tabnas/toml#78). Every segment used to walk the tree from the cell's
// root, so a key of 10,000 segments took three and a half seconds in a
// release build where the other two ports, handed the previous segment's
// table by reference, take tens of milliseconds; now a key moves the
// cursor a header moves, one step per segment. Measured as
// `a_dotted_header_takes_time_in_proportion_to_its_length` is, with the
// guard lifted. Mirrors TestDottedKeyIsLinear in go/perf_test.go and 'a
// dotted key takes time in proportion to its length' in
// ts/test/perf.test.ts.
#[test]
fn a_dotted_key_takes_time_in_proportion_to_its_length() {
    const SHORT: usize = 2_000;
    const LIMIT: u32 = 8;
    let key = |n: usize| format!("{} = 1\n", dotted(n, |_| "a".to_string()));
    let short = key(SHORT);
    let long = key(4 * SHORT);
    let outcome = on_big_stack(move || ratio_of_four_times(unguarded(), short, long, LIMIT))
        .join()
        .expect("the measuring thread finished");
    match outcome {
        Ok((few, many)) => println!(
            "{SHORT} segments={few:?}  {} segments={many:?}  ratio={:.2}x",
            4 * SHORT,
            many.as_secs_f64() / few.as_secs_f64().max(f64::MIN_POSITIVE)
        ),
        Err(shorts) => panic!(
            "a dotted key's parse time grows faster than its length: {SHORT} segments took \
             {shorts:?} in three attempts, and {} segments took more than {LIMIT}x that every \
             time (linear is about 4x, quadratic 16x). Something per segment is walking the \
             path from the root, copying it, or keeping a copy of it.",
            4 * SHORT
        ),
    }
}

// A key thousands of segments long builds exactly the tables it names, and
// later keys walk back down through them: one adds a value beside the
// first key's last segment, one adds a table there, and one that treats a
// segment holding a value as a table is refused, with the same diagnosis a
// short key gets. With the guard lifted. Mirrors TestLongDottedKeyValue in
// go/perf_test.go and 'a long dotted key builds every table it names' in
// ts/test/perf.test.ts.
#[test]
fn a_long_dotted_key_builds_every_table_it_names() {
    on_big_stack(|| {
        const DEPTH: usize = 5_000;
        let path = dotted(DEPTH, |i| format!("k{i}"));
        let prefix = dotted(DEPTH - 1, |i| format!("k{i}"));
        let src = format!("{path} = 1\n{prefix}.y = 2\n{prefix}.z.w = 3\n");
        let parser = unguarded();
        let value = parser.parse(&src).expect("the long keys parse");

        // Walked with a loop: a value 5,000 deep is not compared with the
        // call stack.
        let mut table = &value;
        for depth in 0..DEPTH - 1 {
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
            format!(r#"{{"k{}":1,"y":2,"z":{{"w":3}}}}"#, DEPTH - 1)
        );

        let error = parser
            .parse(&format!("{src}{path}.q = 4\n"))
            .expect_err("the last segment holds a value, so it is not a table to add to");
        assert_eq!("toml_key_conflict", error.code);
        assert!(
            error.to_string().contains(&format!(
                "cannot define k{}, it already has the value 1",
                DEPTH - 1
            )),
            "{error}"
        );
    })
    .join()
    .expect("the long keys were checked to the end");
}

// The depth limit is where the long documents above stop under the guard,
// so a test that lifts it is lifting a real bound and not a missing one.
const _: () = assert!(2_000 > DEPTH_LIMIT);
