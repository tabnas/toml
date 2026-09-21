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
