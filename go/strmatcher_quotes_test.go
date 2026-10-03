// Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// Error COLUMNS after a multi-line string that ends with extra quotes.
//
// Up to two quotes after the closing delimiter belong to the value, so
// `"""x""""` is `x"`, and each of them is a column of the source too.
// tomlStringMatcher owns its column arithmetic (strmatcher_col_test.go and
// strmatcher_row_test.go are the other halves), and it consumed those
// quotes without counting their columns, so every token and every
// diagnostic after such a string was placed one column early per extra
// quote. TypeScript had the same two lines; Rust, whose matcher hands the
// engine a count of characters, was right. The register row
// `a = '''x''''''''''''''`, 1:18 here and in TypeScript against 1:22 in
// Rust, was this defect and not the engine's lookahead, and it closed
// with it.
//
// ts/test/toml.test.ts 'columns after a multi-line string count its extra
// quotes' and columns_after_a_multi_line_string_count_its_extra_quotes in
// rs/tests/toml_test.rs assert the same positions and traces.

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

func TestColumnsAfterAMultiLineStringCountItsExtraQuotes(t *testing.T) {
	for _, c := range []struct {
		label string
		src   string
		row   int
		col   int
	}{
		// Control: no extra quote.
		{"none", "a = \"\"\"x\"\"\" ]", 1, 13},
		{"one", "a = \"\"\"x\"\"\"\" ]", 1, 14},
		{"two", "a = \"\"\"x\"\"\"\"\" ]", 1, 15},
		{"literal, one", "a = '''x'''' ]", 1, 14},
		{"literal, two", "a = '''x''''' ]", 1, 15},
	} {
		_, err, panicked := safeParse(c.src)
		if err == nil {
			t.Errorf("%s: %q parsed, expected a diagnostic", c.label, c.src)
			continue
		}
		if panicked {
			t.Errorf("%s: %q panicked: %v", c.label, c.src, err)
			continue
		}
		b, mErr := json.Marshal(err)
		if mErr != nil {
			t.Fatalf("%s: marshal: %v", c.label, mErr)
		}
		var o struct {
			Row int `json:"row"`
			Col int `json:"col"`
		}
		if uErr := json.Unmarshal(b, &o); uErr != nil {
			t.Fatalf("%s: unmarshal: %v", c.label, uErr)
		}
		if o.Row != c.row || o.Col != c.col {
			t.Errorf("%s: %q reported at %d:%d, want %d:%d. A column short by "+
				"one per extra quote means a quote after the closing delimiter "+
				"is consumed without its column being counted.",
				c.label, c.src, o.Row, o.Col, c.row, c.col)
		}
	}

	trace := func(src string) string {
		j := MakeJsonic()
		var seen []string
		j.Sub(func(tkn *jsonic.Token, _ *jsonic.Rule, _ *jsonic.Context) {
			if tkn.Name == "#ST" || tkn.Name == "#ID" {
				seen = append(seen, fmt.Sprintf("%s%q@%d:%d", tkn.Name, tkn.Src, tkn.RI, tkn.CI))
			}
		}, nil)
		if _, err := j.Parse(src); err != nil {
			t.Fatalf("%q: %v", src, err)
		}
		return strings.Join(seen, " ")
	}
	for _, c := range []struct{ src, want string }{
		{"a = \"\"\"x\"\"\"\"\nb = 1", `#ID"a"@1:1 #ST"\"\"\"x\"\"\"\""@1:13 #ID"b"@2:1`},
		{"a = '''x'''''\nb = 1", `#ID"a"@1:1 #ST"'''x'''''"@1:14 #ID"b"@2:1`},
	} {
		if got := trace(c.src); got != c.want {
			t.Errorf("%q traces as\n  %s\nwant\n  %s", c.src, got, c.want)
		}
	}
}
