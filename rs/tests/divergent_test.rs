// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

// The divergence register: where this repo's ports DISAGREE, executed.
//
// `ts/test/divergent.test.ts` and `go/divergent_test.go` run the SAME
// file and read their own columns; this reads `rust`.
//
// WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES.
// This fails BOTH ways: when a port is repaired to agree with another,
// the row still claims they differ, so the suite goes red and names the
// row to delete. A divergence recorded as a passing test of current
// behaviour survives its own repair, with nothing red, which is how the
// 2026-08 fleet audit found 29 recorded claims contradicted by execution.
//
// WHY THE RUNNER IS LOCAL, as it is in the other two halves.
//
// `tabnas_support::Register` compares two cells by their error CODE and
// drops the `@row:col` suffix. Every row of THIS register is a positional
// disagreement, and all three ports raise `unexpected` on both inputs, so
// that comparator reads every row as "records no divergence" and the file
// asserts nothing. That is the precise failure the Go half's own comment
// describes arriving through a dependency (tabnas/support#12), and the
// reason both other halves parse the cell here rather than asking the
// shared library for it. The cell format is this repo's contract,
// documented in `../test/AGENTS.md`, so this repo reads it.

mod common;

use std::path::Path;

use tabnas_support::{equal_value, is_error_expect, load_spec, parse_expect, SpecOptions};

use common::repo_dir;

/// This runtime's column.
const RUNTIME: &str = "rust";
/// The others, read only to tell a CLOSED divergence from a regression.
const OTHERS: &[&str] = &["ts", "go"];

/// What this port does with one input, in the register's own vocabulary.
fn outcome(src: &str) -> String {
    match tabnas_toml::make().parse(src) {
        Ok(value) => value.to_json().to_string(),
        Err(error) => format!("ERROR:{}@{}:{}", error.code, error.row, error.col),
    }
}

/// `ERROR:unexpected@1:8` as ("unexpected", "1:8"), and `ERROR:unexpected`
/// as ("unexpected", "").
fn split_cell(cell: &str) -> (String, String) {
    let code = cell.strip_prefix("ERROR:").unwrap_or(cell);
    match code.rsplit_once('@') {
        Some((head, position))
            if position.split_once(':').is_some_and(|(row, col)| {
                !row.is_empty()
                    && !col.is_empty()
                    && row.bytes().all(|b| b.is_ascii_digit())
                    && col.bytes().all(|b| b.is_ascii_digit())
            }) =>
        {
            (head.to_string(), position.to_string())
        }
        _ => (code.to_string(), String::new()),
    }
}

/// Whether two cells MEAN the same thing. Compared by meaning, not bytes:
/// `1` and `1.0` are one expectation, and a row whose columns differ only
/// that way records no divergence at all.
fn same_expectation(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if is_error_expect(a) || is_error_expect(b) {
        if !is_error_expect(a) || !is_error_expect(b) {
            return false;
        }
        let (code_a, position_a) = split_cell(a);
        let (code_b, position_b) = split_cell(b);
        if code_a != code_b {
            return false;
        }
        // POSITION IS OPT-IN. A cell that pins no position is satisfied
        // by any position; one that does is compared on both.
        return position_a.is_empty() || position_b.is_empty() || position_a == position_b;
    }
    match (parse_expect(a), parse_expect(b)) {
        (Ok(value_a), Ok(value_b)) => equal_value(&value_a, &value_b),
        _ => false,
    }
}

#[test]
fn divergence_register() {
    let path = repo_dir().join("test").join("divergent.tsv");
    let spec = load_spec(&path, &SpecOptions::default())
        .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.0));

    // An EMPTY register is legitimate, a repo with no divergences, but an
    // empty FILE is not: it cannot be told apart from a loader that read
    // nothing.
    assert!(!spec.rows.is_empty(), "{} has no rows", path.display());

    let mut failures: Vec<String> = Vec::new();
    for row in &spec.rows {
        let input = row.unesc_named("input");
        let mine = row.named(RUNTIME).to_string();
        let others: Vec<(&str, String)> = OTHERS
            .iter()
            .map(|name| (*name, row.named(name).to_string()))
            .collect();
        let where_ = format!("{}:{}", file_name(&path), row.line);

        // 1. Does this row record a divergence at all? Columns that all
        //    say the same thing assert nothing and would pass forever,
        //    which is the shape of the prose claims this replaces.
        if others.iter().all(|(_, cell)| same_expectation(cell, &mine)) {
            failures.push(format!(
                "{where_}: every runtime column means {mine:?}, so this row records no \
                 divergence and can never fail meaningfully. Delete it, or correct the \
                 cells to what the ports actually do."
            ));
            continue;
        }

        let got = outcome(&input);
        if same_expectation(&got, &mine) {
            continue;
        }

        // 2. It changed. Did it change INTO another port's answer? Then
        //    the divergence is closed, and reporting a regression would
        //    send the reader to exactly the wrong conclusion.
        let converged: Vec<&str> = others
            .iter()
            .filter(|(_, cell)| same_expectation(&got, cell))
            .map(|(name, _)| *name)
            .collect();
        if converged.is_empty() {
            // 3. Neither. An ordinary regression.
            failures.push(format!(
                "{where_}: {RUNTIME} changed, and not into another port's answer either, \
                 so this is a regression, not a closed divergence.\n  got:      {got}\n  \
                 expected: {mine}"
            ));
            continue;
        }
        failures.push(format!(
            "{where_}: this divergence is CLOSED against {}. {RUNTIME} now produces {got}, \
             not its own {mine}.\n  A fixed divergence fails as loudly as a regressed one, \
             so the row cannot outlive it.\n  Update or DELETE this row, and if the repair \
             landed in the engine, check the other rows citing {}.",
            converged.join(", "),
            row.named("why").trim()
        ));
    }

    assert!(
        failures.is_empty(),
        "{} divergence row(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The comparator itself, because a comparator that stops distinguishing
/// positions does not fail: it makes every positional row read as "records
/// no divergence", and the register becomes a file of rows that assert
/// nothing while staying green on the rows that survive.
///
/// That is not hypothetical here. It is exactly what
/// `tabnas_support::Register` does today, which is why this file carries
/// its own comparator. `go/divergent_test.go` and
/// `ts/test/divergent.test.ts` assert the same four cases.
#[test]
fn same_expectation_reads_the_position() {
    for (name, a, b, same) in [
        // The case that regressed: same code, different column.
        (
            "differing column",
            "ERROR:unexpected@1:5",
            "ERROR:unexpected@1:6",
            false,
        ),
        (
            "differing row",
            "ERROR:unexpected@1:5",
            "ERROR:unexpected@2:5",
            false,
        ),
        // Controls. Without these, "distinguishes positions" is also
        // satisfied by a comparator that calls everything different.
        (
            "identical position",
            "ERROR:unexpected@1:5",
            "ERROR:unexpected@1:5",
            true,
        ),
        (
            "position is opt-in",
            "ERROR:unexpected",
            "ERROR:unexpected@1:5",
            true,
        ),
    ] {
        assert_eq!(
            same_expectation(a, b),
            same,
            "{name}: same_expectation({a:?}, {b:?})"
        );
    }
}
