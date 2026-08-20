// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// divergent_test.go — the divergence register: where this repo's two ports
// DISAGREE, executed.
//
// ts/test/divergent.test.ts runs the SAME file and reads the other column.
//
// WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES. This
// fails BOTH ways: when a port is repaired to agree with the other, the row
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

// This runtime's column. The TypeScript half reads `ts`.
const (
	registerRuntime = "go"
	registerOther   = "ts"
)

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
		theirs := row.Named(registerOther)

		t.Run(fmt.Sprintf("row %d: %q", row.Line, input), func(t *testing.T) {
			// 1. Does this row record a divergence at all? Two columns
			//    saying the same thing assert nothing and would pass
			//    forever, which is the shape of the prose claims this
			//    replaces.
			if sameExpectation(mine, theirs) {
				t.Fatalf("%s: both columns mean %q, so this row records no "+
					"divergence and can never fail meaningfully. Delete it, "+
					"or correct the cells to what the ports actually do.",
					row.Where(), mine)
			}

			got := outcome(input)

			if sameExpectation(got, mine) {
				return
			}

			// 2. It changed. Did it change INTO the other port's answer?
			//    Then the divergence is closed, and reporting a regression
			//    would send the reader to exactly the wrong conclusion.
			if sameExpectation(got, theirs) {
				t.Fatalf("%s: this divergence is CLOSED. %s now produces "+
					"what the %s column records (%s), not its own (%s).\n"+
					"  A fixed divergence fails as loudly as a regressed "+
					"one, so the row cannot outlive it.\n"+
					"  DELETE this row — and if the repair landed in the "+
					"engine, check whether the other rows citing %s go with "+
					"it.",
					row.Where(), registerRuntime, registerOther, theirs, mine,
					strings.TrimSpace(row.Named("why")))
			}

			// 3. Neither. An ordinary regression.
			t.Fatalf("%s: %s changed, and not into the %s answer either — "+
				"this is a regression, not a closed divergence.\n"+
				"  got:      %s\n  expected: %s",
				row.Where(), registerRuntime, registerOther, got, mine)
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
