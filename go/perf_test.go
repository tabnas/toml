package tabnastoml

import (
	"encoding/json"
	"errors"
	"fmt"
	"runtime"
	"strings"
	"testing"
	"time"

	jsonic "github.com/tabnas/jsonic/go"
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

// tomlDottedHeader is a dotted table header of n segments, [a.a.….a], with
// one key in its table.
func tomlDottedHeader(n int) string {
	return "[" + strings.TrimSuffix(strings.Repeat("a.", n), ".") + "]\nx = 1\n"
}

// TestDottedHeaderIsLinear: a dotted table header takes time in proportion
// to its number of segments. The Rust port used to walk the tree from its
// root on every segment, copy the whole path several times over and keep
// one more copy of it per segment, so its time grew faster than the square
// of the header's length (tabnas/toml#81: 10,000 segments took two
// minutes). This port hands each segment the table the previous one
// reached, as r.Prev.Node, which is constant work; the test keeps it that
// way. Mirrors a_dotted_header_takes_time_in_proportion_to_its_length in
// rs/tests/perf_test.rs and ts/test/perf.test.ts.
//
// Machine-independent like the test above: it compares a header four times
// as long, in the same run. Linear time makes that about 4x; quadratic
// makes it 16x. The limit, 8x, sits between. Each parse starts after a
// collection, so the garbage of the one before is not collected inside
// the clock, and a burst of load on a shared runner can still land on
// either side, so each side is the fastest of five parses and the
// comparison gets three attempts.
func TestDottedHeaderIsLinear(t *testing.T) {
	const short = 2000
	j := MakeJsonic()
	timeOf := func(src string) time.Duration {
		best := time.Duration(0)
		for run := 0; run < 5; run++ {
			runtime.GC()
			t0 := time.Now()
			if _, err := j.Parse(src); err != nil {
				t.Fatalf("the header does not parse: %v", err)
			}
			if took := time.Since(t0); run == 0 || took < best {
				best = took
			}
		}
		return best
	}
	few := tomlDottedHeader(short)
	many := tomlDottedHeader(4 * short)
	timeOf(few)
	timeOf(many)
	var seen []string
	for attempt := 0; attempt < 3; attempt++ {
		fewTime := timeOf(few)
		manyTime := timeOf(many)
		ratio := float64(manyTime) / float64(max(fewTime, 1))
		seen = append(seen, fmt.Sprintf("%v and %v (%.1fx)", fewTime, manyTime, ratio))
		if ratio <= 8 {
			t.Logf("%d segments=%v  %d segments=%v  ratio=%.2fx", short, fewTime, 4*short, manyTime, ratio)
			return
		}
	}
	t.Errorf("a dotted header's parse time grows faster than its length: %d and %d segments "+
		"took %s (linear is about 4x, quadratic 16x). Something per segment is walking the "+
		"path from the root, copying it, or keeping a copy of it.",
		short, 4*short, strings.Join(seen, ", then "))
}

// TestLongDottedHeaderValue: a header thousands of segments long builds
// exactly the tables it names, and later headers walk back down through
// them: one adds a table beside the first one's key, two arrays of tables
// append to the same array, and one that treats a key holding a value as a
// table is refused, with the same diagnosis a short header gets. Mirrors
// a_long_dotted_header_builds_every_table_it_names in rs/tests/perf_test.rs
// and ts/test/perf.test.ts.
func TestLongDottedHeaderValue(t *testing.T) {
	const depth = 5000
	keys := make([]string, depth)
	for i := range keys {
		keys[i] = fmt.Sprintf("k%d", i)
	}
	path := strings.Join(keys, ".")
	src := fmt.Sprintf("[%[1]s]\nx = 1\n[%[1]s.y]\nz = 2\n[[%[1]s.list]]\nn = 1\n[[%[1]s.list]]\nn = 2\n", path)
	table, err := Parse(src)
	if err != nil {
		t.Fatalf("the long headers do not parse: %v", err)
	}
	// Walked with a loop, level by level.
	for level, key := range keys {
		m, ok := table.(*jsonic.OrderedMap)
		if !ok || len(m.Keys) != 1 || m.Keys[0] != key {
			t.Fatalf("level %d should hold %s and nothing else, and holds %v", level, key, table)
		}
		table, _ = m.Get(key)
	}
	got, err := json.Marshal(table)
	if err != nil {
		t.Fatalf("marshal the innermost table: %v", err)
	}
	if want := `{"x":1,"y":{"z":2},"list":[{"n":1},{"n":2}]}`; string(got) != want {
		t.Errorf("the innermost table is %s, want %s", got, want)
	}

	_, err = Parse(src + fmt.Sprintf("[%s.x.q]\n", path))
	var te *jsonic.JsonicError
	if !errors.As(err, &te) || te.Code != "toml_key_conflict" {
		t.Fatalf("x holds a value, so it is not a table to add to; got %v", err)
	}
	if want := "cannot define x, it already has the value 1"; !strings.Contains(te.Detail, want) {
		t.Errorf("the refusal says %q, want %q", te.Detail, want)
	}
}

// tomlDottedKey is a dotted key of n segments, a.a.….a = 1.
func tomlDottedKey(n int) string {
	return strings.TrimSuffix(strings.Repeat("a.", n), ".") + " = 1\n"
}

// deepestRule is the engine's rule depth D the deepest rule of a parse
// reaches, read through a rule subscriber.
func deepestRule(t *testing.T, src string) int {
	t.Helper()
	j := MakeJsonic()
	deepest := 0
	j.Sub(nil, func(r *jsonic.Rule, _ *jsonic.Context) {
		if r.D > deepest {
			deepest = r.D
		}
	})
	if _, err := j.Parse(src); err != nil {
		t.Fatalf("%.40q…: %v", src, err)
	}
	return deepest
}

// TestDottedKeyRuleDepthIsConstant: a dotted key is a replace loop
// (tabnas/toml#78). Each segment ending in a dot re-enters `dive` in the
// same frame, so the engine's rule depth D stays what one segment needs
// however long the key. It used to push a dive per segment, so D was the
// segment count plus two, 10,002 for ten thousand segments, past the
// 3,000 open rules aless allows a parse. A header, which was always a
// loop, is the control. Mirrors 'a dotted key keeps rule depth constant'
// in ts/test/perf.test.ts and a_dotted_key_keeps_rule_depth_constant in
// rs/tests/perf_test.rs.
func TestDottedKeyRuleDepthIsConstant(t *testing.T) {
	two := deepestRule(t, tomlDottedKey(2))
	many := deepestRule(t, tomlDottedKey(10000))
	if many != two {
		t.Errorf("ten thousand segments reach rule depth %d, where two reach %d: "+
			"the dive is a push chain again", many, two)
	}
	if h2, h := deepestRule(t, tomlDottedHeader(2)), deepestRule(t, tomlDottedHeader(10000)); h != h2 {
		t.Errorf("a header's rule depth grew with its length: %d against %d", h, h2)
	}
}

// TestDottedKeyIsLinear: a dotted key takes time in proportion to its
// number of segments. This port hands each segment the table the previous
// one reached, as r.Prev.Node, which is constant work; the Rust port used
// to walk the tree from its root on every segment (tabnas/toml#78).
// Measured as TestDottedHeaderIsLinear is. Mirrors 'a dotted key takes
// time in proportion to its length' in ts/test/perf.test.ts and
// a_dotted_key_takes_time_in_proportion_to_its_length in
// rs/tests/perf_test.rs.
func TestDottedKeyIsLinear(t *testing.T) {
	const short = 2000
	j := MakeJsonic()
	timeOf := func(src string) time.Duration {
		best := time.Duration(0)
		for run := 0; run < 5; run++ {
			runtime.GC()
			t0 := time.Now()
			if _, err := j.Parse(src); err != nil {
				t.Fatalf("the key does not parse: %v", err)
			}
			if took := time.Since(t0); run == 0 || took < best {
				best = took
			}
		}
		return best
	}
	few := tomlDottedKey(short)
	many := tomlDottedKey(4 * short)
	timeOf(few)
	timeOf(many)
	var seen []string
	for attempt := 0; attempt < 3; attempt++ {
		fewTime := timeOf(few)
		manyTime := timeOf(many)
		ratio := float64(manyTime) / float64(max(fewTime, 1))
		seen = append(seen, fmt.Sprintf("%v and %v (%.1fx)", fewTime, manyTime, ratio))
		if ratio <= 8 {
			t.Logf("%d segments=%v  %d segments=%v  ratio=%.2fx", short, fewTime, 4*short, manyTime, ratio)
			return
		}
	}
	t.Errorf("a dotted key's parse time grows faster than its length: %d and %d segments "+
		"took %s (linear is about 4x, quadratic 16x). Something per segment is walking the "+
		"path from the root, copying it, or keeping a copy of it.",
		short, 4*short, strings.Join(seen, ", then "))
}

// TestLongDottedKeyValue: a key thousands of segments long builds exactly
// the tables it names, and later keys walk back down through them: one
// adds a value beside the first key's last segment, one adds a table
// there, and one that treats a segment holding a value as a table is
// refused, with the same diagnosis a short key gets. Mirrors 'a long
// dotted key builds every table it names' in ts/test/perf.test.ts and
// a_long_dotted_key_builds_every_table_it_names in rs/tests/perf_test.rs.
func TestLongDottedKeyValue(t *testing.T) {
	const depth = 5000
	keys := make([]string, depth)
	for i := range keys {
		keys[i] = fmt.Sprintf("k%d", i)
	}
	path := strings.Join(keys, ".")
	prefix := strings.Join(keys[:depth-1], ".")
	src := fmt.Sprintf("%s = 1\n%s.y = 2\n%s.z.w = 3\n", path, prefix, prefix)
	table, err := Parse(src)
	if err != nil {
		t.Fatalf("the long keys do not parse: %v", err)
	}
	// Walked with a loop, level by level.
	for level, key := range keys[:depth-1] {
		m, ok := table.(*jsonic.OrderedMap)
		if !ok || len(m.Keys) != 1 || m.Keys[0] != key {
			t.Fatalf("level %d should hold %s and nothing else, and holds %v", level, key, table)
		}
		table, _ = m.Get(key)
	}
	got, err := json.Marshal(table)
	if err != nil {
		t.Fatalf("marshal the innermost table: %v", err)
	}
	if want := fmt.Sprintf(`{"k%d":1,"y":2,"z":{"w":3}}`, depth-1); string(got) != want {
		t.Errorf("the innermost table is %s, want %s", got, want)
	}

	_, err = Parse(src + fmt.Sprintf("%s.q = 4\n", path))
	var te *jsonic.JsonicError
	if !errors.As(err, &te) || te.Code != "toml_key_conflict" {
		t.Fatalf("the last segment holds a value, so it is not a table to add to; got %v", err)
	}
	if want := fmt.Sprintf("cannot define k%d, it already has the value 1", depth-1); !strings.Contains(te.Detail, want) {
		t.Errorf("the refusal says %q, want %q", te.Detail, want)
	}
}
