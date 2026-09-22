// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// divergent_test.go — the divergence register: where this repo's ports
// DISAGREE, executed.
//
// ts/test/divergent.test.ts and rs/tests/divergent_test.rs run the SAME
// file and read their own columns.
//
// WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES. This
// fails BOTH ways: when a port is repaired to agree with another, the row
// still claims they differ, so the suite goes red and names the row to
// delete. A divergence recorded as a passing test of current behaviour
// survives its own repair — the port is fixed, the test is updated, and the
// record now describes something that no longer happens, with nothing red.
// That is how the 2026-08 fleet audit found 29 recorded claims contradicted
// by execution.
//
// The runner is LOCAL for now. github.com/tabnas/support/go gains this
// mechanism in tabnas/support#14, and the row vocabulary here is
// deliberately the one that PR standardises — including @<row>:<col> from
// support#12 — so adopting it deletes this file and leaves the fixture
// untouched.

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
	support "github.com/tabnas/support/go"
)

// registerRuntime is this runtime's column. The TypeScript half reads
// `ts` and the Rust half `rust`.
const registerRuntime = "go"

// registerOthers is EVERY other runtime column, not one of them. A
// register with a column per runtime can record a divergence between any
// pair, so a row where only `rust` differs is a divergence this half must
// accept rather than reject as vacuous. Reading a single other column made
// such a row impossible to write.
var registerOthers = []string{"ts", "rust"}

// outcome is what this port did with one input, in the register's own
// vocabulary.
func outcome(src string) string {
	// This port exposes a package-level Parse that builds its own engine —
	// a different plugin surface from json5's Parse(j, src). The probe in
	// admin assumed json5's and could not compile against this one, which
	// is why this repo had never been probed at all.
	v, err := Parse(src)
	if nil != err {
		if te, ok := err.(*jsonic.JsonicError); ok {
			return fmt.Sprintf("ERROR:%s@%d:%d", te.Code, te.Row, te.Col)
		}
		return "ERROR:?"
	}

	b, merr := json.Marshal(v)
	if nil != merr {
		return "UNMARSHALABLE"
	}
	return string(b)
}

// splitCell reads `ERROR:unexpected@1:8` as ("unexpected", "1:8") and
// `ERROR:unexpected` as ("unexpected", "").
//
// PARSED HERE, NOT BY THE SUPPORT LIBRARY, and that is deliberate. This
// comparator used to ask `support.ErrorCode` for the code and then look
// for an `@1:8` suffix on the answer. tabnas/support#12 split the
// position OUT of that value, so the suffix was no longer there to find:
// every cell parsed as "pins no position", every pair of rows sharing a
// code compared EQUAL, and this whole register silently became rows that
// assert nothing — the exact failure it exists to catch, arriving through
// a dependency rather than an edit.
//
// The obvious repair — call the newer `support.ErrorExpect` and read
// Row/Col off the struct — swaps one coupling for another AND does not
// compile against the `support/go v0.3.1` this module pins, because that
// function does not exist there. Green CI hid it: the shared polyglot-ci
// workflow links the SIBLING checkout, where it does exist, so `go test`
// passed in CI and failed for anyone building against the pinned release.
//
// The cell format is this repo's own contract, documented in
// test/AGENTS.md, so this repo reads it. No support version can change
// what it means.
var cellPosition = regexp.MustCompile(`@(\d+:\d+)$`)

func splitCell(cell string) (string, string) {
	code := strings.TrimPrefix(cell, "ERROR:")
	if loc := cellPosition.FindStringSubmatchIndex(code); nil != loc {
		return code[:loc[0]], code[loc[2]:loc[3]]
	}
	return code, ""
}

// sameExpectation reports whether two cells MEAN the same thing. Compared
// by meaning, not bytes: `1` and `1.0` are one expectation, and a row whose
// columns differ only that way records no divergence at all.
func sameExpectation(a, b string) bool {
	if a == b {
		return true
	}

	if support.IsErrorExpect(a) || support.IsErrorExpect(b) {
		if !support.IsErrorExpect(a) || !support.IsErrorExpect(b) {
			return false
		}
		ca, pa := splitCell(a)
		cb, pb := splitCell(b)
		if ca != cb {
			return false
		}
		// POSITION IS OPT-IN. A cell that pins no position is satisfied by
		// any position; one that does is compared on both. Same rule as
		// tabnas/support#12.
		return "" == pa || "" == pb || pa == pb
	}

	va, erra := support.ParseExpect(a)
	vb, errb := support.ParseExpect(b)
	if nil != erra || nil != errb {
		return false
	}
	return support.EqualValue(va, vb)
}

func TestDivergenceRegister(t *testing.T) {
	dir, err := support.FindSpecDir("")
	if nil != err {
		t.Fatalf("%v", err)
	}
	path := filepath.Join(dir, "..", "divergent.tsv")

	spec, err := support.LoadSpec(path, nil)
	if nil != err {
		t.Fatalf("%v", err)
	}

	// An EMPTY register is legitimate — a repo with no divergences — but an
	// empty FILE is not: it cannot be told apart from a loader that read
	// nothing.
	if 0 == len(spec.Rows) {
		t.Fatalf("%s has no rows", path)
	}

	for _, row := range spec.Rows {
		input := row.UnescNamed("input")
		mine := row.Named(registerRuntime)
		others := make(map[string]string, len(registerOthers))
		for _, name := range registerOthers {
			others[name] = row.Named(name)
		}

		t.Run(fmt.Sprintf("row %d: %q", row.Line, input), func(t *testing.T) {
			// 1. Does this row record a divergence at all? Columns that
			//    all say the same thing assert nothing and would pass
			//    forever, which is the shape of the prose claims this
			//    replaces.
			//
			//    Judged over EVERY PAIR of cells, not each other cell
			//    against this runtime's. A cell that pins no position is
			//    satisfied by any position, so it is a wildcard: with ts
			//    `ERROR:x`, go `ERROR:x@1:1` and rust `ERROR:x@1:2`, the
			//    old comparison answered "vacuous" in the TypeScript half
			//    and "not vacuous" here, for one row. The three suites
			//    have to agree about whether a row records anything, and
			//    a real disagreement between two OTHER runtimes is one.
			cells := make([]string, 0, len(others)+1)
			cells = append(cells, mine)
			for _, name := range registerOthers {
				cells = append(cells, others[name])
			}
			vacuous := true
			for i := 0; i < len(cells) && vacuous; i++ {
				for j := i + 1; j < len(cells); j++ {
					if !sameExpectation(cells[i], cells[j]) {
						vacuous = false
						break
					}
				}
			}
			if vacuous {
				t.Fatalf("%s: every runtime column means %q, so this row "+
					"records no divergence and can never fail meaningfully. "+
					"Delete it, or correct the cells to what the ports "+
					"actually do.",
					row.Where(), mine)
			}

			got := outcome(input)

			if sameExpectation(got, mine) {
				return
			}

			// 2. It changed. Did it change INTO another port's answer?
			//    Then the divergence is closed, and reporting a regression
			//    would send the reader to exactly the wrong conclusion.
			converged := make([]string, 0, len(registerOthers))
			for _, name := range registerOthers {
				if sameExpectation(got, others[name]) {
					converged = append(converged, name)
				}
			}
			if 0 < len(converged) {
				t.Fatalf("%s: this divergence is CLOSED against %s. %s now "+
					"produces %s, not its own %s.\n"+
					"  A fixed divergence fails as loudly as a regressed "+
					"one, so the row cannot outlive it.\n"+
					"  Update or DELETE this row, and if the repair landed "+
					"in the engine, check the other rows citing %s.",
					row.Where(), strings.Join(converged, ", "),
					registerRuntime, got, mine,
					strings.TrimSpace(row.Named("why")))
			}

			// 3. Neither. An ordinary regression.
			t.Fatalf("%s: %s changed, and not into another port's answer "+
				"either, so this is a regression, not a closed divergence.\n"+
				"  got:      %s\n  expected: %s",
				row.Where(), registerRuntime, got, mine)
		})
	}
}

// TestSameExpectationReadsThePosition pins the comparator itself, because a
// comparator that stops distinguishing positions does not fail — it makes
// every position row in the register read as "records no divergence", and
// the register becomes a file of rows that assert nothing while staying
// green on the rows that survive.
//
// That is not hypothetical. `sameExpectation` used to re-parse an `@1:8`
// suffix off `support.ErrorCode`, and tabnas/support#12 split the position
// OUT of that value — so every cell parsed as "pins no position" and every
// pair sharing a code compared equal. Nothing in this repo asserted the
// comparator's own behaviour, so the only symptom was the vacuity check
// firing on rows that were, in fact, perfectly good.
//
// ts/test/divergent.test.ts asserts the same four cases.
func TestSameExpectationReadsThePosition(t *testing.T) {
	for _, c := range []struct {
		name, a, b string
		same       bool
	}{
		// The case that regressed: same code, different column.
		{"differing column", "ERROR:unexpected@1:5", "ERROR:unexpected@1:6", false},
		{"differing row", "ERROR:unexpected@1:5", "ERROR:unexpected@2:5", false},

		// Controls. Without these, "distinguishes positions" is also
		// satisfied by a comparator that calls everything different.
		{"identical position", "ERROR:unexpected@1:5", "ERROR:unexpected@1:5", true},
		{"position is opt-in", "ERROR:unexpected", "ERROR:unexpected@1:5", true},
	} {
		if got := sameExpectation(c.a, c.b); got != c.same {
			t.Errorf("%s: sameExpectation(%q, %q) = %v, want %v",
				c.name, c.a, c.b, got, c.same)
		}
	}
}
