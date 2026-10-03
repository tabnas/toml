// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

// TestKeyConflictIsDiagnosed: a key TOML does not allow to be redefined is
// refused with `toml_key_conflict`, the code the TypeScript and Rust ports
// answer for the same documents (ts/test/toml.test.ts
// key-conflict-is-diagnosed; rs/tests/toml_test.rs). Every input here was
// measured against the TypeScript port before this test was written.
//
// This port used to ACCEPT every one of them and lose data doing it: a
// value redefined as a table was overwritten by an empty table, and an
// array of tables redefined as a table was replaced by the second table,
// dropping the first. Nothing reported either. The nineteen documents of
// the BurntSushi corpus in this class are what moved the `go` row of
// test/conformance.tsv.
//
// The code is asserted, not merely the failure: the engine turns an
// arbitrary panic inside an action into `internal`, and an `internal`
// error here would be the crash this diagnosis replaces.
func TestKeyConflictIsDiagnosed(t *testing.T) {
	cases := []struct {
		src  string
		want string // the detail the TypeScript port renders
		at   string // row:col of the key being redefined
	}{
		// A value used again as a table, through a dotted key in an inline
		// table and through a header.
		{"a = {b = 1, b.c = 2}", "cannot define b, it already has the value 1", "1:13"},
		{`a = {b = "s", b.c = 2}`, `cannot define b, it already has the value "s"`, "1:15"},
		{"a = 1\n[a]\nb = 2", "cannot define a, it already has the value 1", "2:2"},
		{"a = 1\n[a.b]\nc = 2", "cannot define a, it already has the value 1", "2:2"},
		{"a = 1\n[a.b.c]\nd = 2", "cannot define a, it already has the value 1", "2:2"},
		{"[a]\nb = 1\n[a.b]\nc = 2", "cannot define b, it already has the value 1", "3:4"},
		{"[a]\nb = 1\n[a.b.c]\nd = 2", "cannot define b, it already has the value 1", "3:4"},
		{"[t]\na = 1\n[t]\n[t.a]\nb = 2", "cannot define a, it already has the value 1", "4:4"},

		// A value used again as a table-array header.
		{"a = 1\n[[a]]\nb = 2", "cannot define a, it already has the value 1", "2:3"},
		{"a = 1\n[[a.b]]\nc = 2", "cannot define a, it already has the value 1", "2:3"},
		{"[a]\nb = 1\n[[a.b]]\nc = 2", "cannot define b, it already has the value 1", "3:5"},
		{"[[a]]\nb = 1\n[[a.b]]\nc = 2", "cannot define b, it already has the value 1", "3:5"},

		// A table redefined as an array of tables.
		{"[a]\n[[a]]", "cannot define a, it already has the value {}", "2:3"},

		// An array of tables redefined as a table. These never crashed
		// anywhere: both older ports accepted them and destroyed data, in
		// opposite directions. corpus: array/tables-02,
		// table/duplicate-key-07.
		{"[[x]]\na = 1\n[x]\nb = 2", "cannot define x, it is already an array of tables", "3:2"},
		{"[x]\n[[x.y]]\n[[x.y]]\n[x.y]", "cannot define y, it is already an array of tables", "4:4"},
		{"[[fruit]]\nname = \"apple\"\n[[fruit.variety]]\nname = \"red delicious\"\n" +
			"[fruit.variety]\nname = \"granny smith\"",
			"cannot define variety, it is already an array of tables", "5:8"},
	}

	for _, c := range cases {
		_, err := Parse(c.src)
		if err == nil {
			t.Errorf("%q: accepted, expected toml_key_conflict", c.src)
			continue
		}
		var te *jsonic.JsonicError
		if !errors.As(err, &te) {
			t.Errorf("%q: rejected with %T, not a coded error: %v", c.src, err, err)
			continue
		}
		if te.Code != "toml_key_conflict" {
			t.Errorf("%q: rejected as %s, want toml_key_conflict (an `internal` here is "+
				"the crash this diagnosis replaces): %v", c.src, te.Code, err)
			continue
		}
		if !strings.Contains(te.Detail, c.want) {
			t.Errorf("%q: detail %q does not say %q", c.src, te.Detail, c.want)
		}
		// The hint registered for the code reaches the reader, which is
		// what tells a template-less "unknown error" from a diagnosis.
		if !strings.Contains(te.Hint, "TOML does not allow a key to be redefined") {
			t.Errorf("%q: hint %q is not the toml_key_conflict hint", c.src, te.Hint)
		}
		// The error points at the key being redefined, as the Rust port's
		// does. The TypeScript port answers 1:1: it raises on `ctx.t0`,
		// which its engine has emptied by the time an action runs, and
		// that position is the row the register records for this class.
		if at := fmt.Sprintf("%d:%d", te.Row, te.Col); at != c.at {
			t.Errorf("%q: at %s, want %s (the key being redefined)", c.src, at, c.at)
		}
	}
}

// TestImplicitTablesStayLegal: descending into an existing table, into an
// implicit table a dotted key or a deeper header created, or into the last
// element of an existing array of tables is legitimate and must NOT read
// as a conflict. Four valid corpus documents do exactly this, and a first
// cut of the TypeScript check rejected all four. The expected values are
// the TypeScript port's.
func TestImplicitTablesStayLegal(t *testing.T) {
	cases := []struct{ src, want string }{
		{"a = {b = 1, c = 2}", `{"a":{"b":1,"c":2}}`},
		{"a = {b.c = 1, b.d = 2}", `{"a":{"b":{"c":1,"d":2}}}`},
		// A `[table]` header extends the implicit table a dotted key or a
		// deeper header made; a dotted key extends the table a header made.
		{"a.b = 1\n[a]\nc = 2", `{"a":{"b":1,"c":2}}`},
		{"[a.b]\nc = 1\n[a]\nd = 2", `{"a":{"b":{"c":1},"d":2}}`},
		{"[a]\nb.c = 1\n[a.b]\nd = 2", `{"a":{"b":{"c":1,"d":2}}}`},
		// Through an array of tables to its last element.
		{"[[x]]\ny = 1\n[x.z]\nw = 2", `{"x":[{"y":1,"z":{"w":2}}]}`},
		{"[[a]]\n[[a]]\n[a.b]\nc = 1", `{"a":[{},{"b":{"c":1}}]}`},
		// A table defined twice passes through, as it does in TypeScript.
		{"[a]\n[a]", `{"a":{}}`},
	}
	for _, c := range cases {
		v, err := Parse(c.src)
		if err != nil {
			t.Errorf("%q: must still parse, got %v", c.src, err)
			continue
		}
		got, merr := json.Marshal(v)
		if merr != nil {
			t.Errorf("%q: result does not marshal: %v", c.src, merr)
			continue
		}
		if string(got) != c.want {
			t.Errorf("%q:\n  got:  %s\n  want: %s", c.src, got, c.want)
		}
	}
}
