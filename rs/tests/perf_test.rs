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

use std::time::Instant;

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
