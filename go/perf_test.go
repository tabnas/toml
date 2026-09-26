package tabnastoml

import (
	"fmt"
	"strings"
	"testing"
	"time"
)

// TestParseReusesInstance guards against a performance regression where the
// convenience Parse() rebuilds the (expensive) TOML grammar on every call
// instead of reusing a cached instance. Building the grammar dominates a
// parse, so a rebuild-per-call Parse() is many times slower than reusing one
// MakeJsonic() instance.
//
// The check is machine-INDEPENDENT: it compares Parse() against instance
// reuse on the SAME machine in the SAME run, so a slow CI box cannot make it
// flaky (both sides scale together). There is deliberately NO wall-clock
// budget.
func TestParseReusesInstance(t *testing.T) {
	const src = "a = 1\nb = 2\nc = 3"
	const n = 3000

	// Warm both paths so the comparison is steady-state.
	for i := 0; i < 100; i++ {
		_, _ = Parse(src)
	}
	j := MakeJsonic()
	for i := 0; i < 100; i++ {
		_, _ = j.Parse(src)
	}

	t0 := time.Now()
	for i := 0; i < n; i++ {
		if _, err := Parse(src); err != nil {
			t.Fatalf("Parse error: %v", err)
		}
	}
	conv := time.Since(t0)

	t1 := time.Now()
	for i := 0; i < n; i++ {
		if _, err := j.Parse(src); err != nil {
			t.Fatalf("reuse parse error: %v", err)
		}
	}
	reuse := time.Since(t1)

	// A cached Parse() is ~= instance reuse; allow 4x for scheduling noise.
	// A rebuild-per-call Parse() is many times slower here, so this catches
	// the regression without depending on absolute wall-clock speed.
	if conv > 4*reuse {
		t.Errorf("Parse() appears to rebuild the grammar on every call: "+
			"%d Parse() calls took %v vs %v reusing one instance (ratio %.1fx, limit 4x). "+
			"Cache a lazy default instance (see Parse / sync.Once).",
			n, conv, reuse, float64(conv)/float64(reuse))
	}
	t.Logf("Parse()=%v  reuse=%v  ratio=%.2fx", conv, reuse, float64(conv)/float64(reuse))
}

// tomlTables is TOML of n array tables, each with strings in it.
func tomlTables(n int) string {
	var b strings.Builder
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "[[item]]\nid = %d\nname = \"item %d\"\ntags = [\"a\", \"b\"]\n\n", i, i)
	}
	return b.String()
}

// TestParseIsLinear: a parse takes time in proportion to the length of the
// document. The Rust port's string matcher used to copy the whole of the
// rest of the source at every token, so its parse time grew with the SQUARE
// of the length (4,000 tables took 23 seconds rather than 0.4). This port
// indexes the source in place; the test keeps it that way. Mirrors
// parse_time_is_linear_in_document_length in rs/tests/perf_test.rs and
// ts/test/perf.test.ts.
//
// Machine-independent like the test above: it compares a document with
// four times as much in it, in the same run. Linear time makes that about
// 4x; quadratic makes it 16x. The limit, 8x, sits between.
func TestParseIsLinear(t *testing.T) {
	const small = 250
	j := MakeJsonic()
	timeOf := func(src string) time.Duration {
		best := time.Duration(0)
		for run := 0; run < 3; run++ {
			t0 := time.Now()
			if _, err := j.Parse(src); err != nil {
				t.Fatalf("the tables do not parse: %v", err)
			}
			if took := time.Since(t0); run == 0 || took < best {
				best = took
			}
		}
		return best
	}
	few := timeOf(tomlTables(small))
	many := timeOf(tomlTables(4 * small))
	ratio := float64(many) / float64(max(few, 1))
	if ratio > 8 {
		t.Errorf("parse time grows faster than document length: %d tables took %v and %d took %v "+
			"(ratio %.1fx; linear is about 4x, quadratic 16x). Something per token is reading "+
			"the rest of the source.", small, few, 4*small, many, ratio)
	}
	t.Logf("%d tables=%v  %d tables=%v  ratio=%.2fx", small, few, 4*small, many, ratio)
}
