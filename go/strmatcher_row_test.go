// Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// Error ROWS after a multi-line string (tabnas/toml#85).
//
// A multi-line string trims the line feed right after its opening
// delimiter, and a line-ending backslash trims the one right after it;
// both are still lines of the source. tomlStringMatcher owns its row and
// column arithmetic (strmatcher_col_test.go is the columns' half), and it
// consumed each of those line feeds without counting a row, so every token
// and every diagnostic after such a string was reported one row early, per
// multi-line string before it. TypeScript had the same two sites wrong;
// Rust, whose matcher hands the engine a count of characters, was right.
//
// ts/test/toml.test.ts 'rows after a multi-line string count its trimmed
// line feeds' and rows_after_a_multi_line_string_count_its_trimmed_line_feeds
// in rs/tests/toml_test.rs assert the same rows, and the three ports assert
// the same six token traces.

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

func TestRowsAfterAMultiLineStringCountItsTrimmedLineFeeds(t *testing.T) {
	for _, c := range []struct {
		label string
		src   string
		row   int
		col   int
	}{
		// Control: a multi-line string that trims nothing.
		{"no trim", "a = \"\"\"x\"\"\"\nb = ]", 2, 5},
		{"basic", "a = \"\"\"\nx\"\"\"\nb = ]", 3, 5},
		{"literal", "a = '''\nx'''\nb = ]", 3, 5},
		// One row early PER string before the error.
		{"two strings", "a = \"\"\"\nx\"\"\"\nb = \"\"\"\ny\"\"\"\nc = ]", 5, 5},
		// The line feed a line-ending backslash trims, then one it trims
		// after that.
		{"backslash", "a = \"\"\"\nx\\\n  y\"\"\"\nb = ]", 4, 5},
		{"backslash, blank line", "a = \"\"\"\nx\\\n\n  y\"\"\"\nb = ]", 5, 5},
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
			t.Errorf("%s: %q reported at %d:%d, want %d:%d. A row short by one "+
				"per multi-line string means a trimmed line feed is consumed "+
				"without its row being counted.",
				c.label, c.src, o.Row, o.Col, c.row, c.col)
		}
	}
}

// The tokens themselves, as the lex trace a highlighter reads them: a
// string token's point is the cursor AFTER the string, so it sits on the
// row the string ends on, and the token after it starts on the next row.
// The empty string carries its two quote characters as its source, as
// every other string token carries its text. TypeScript and Rust assert
// the same six traces.
func TestStringTokensCarryTheirSourceAndEndOnTheRowTheyEndOn(t *testing.T) {
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
		{"a = \"\"\"\nx\"\"\"\nb = 1", `#ID"a"@1:1 #ST"\"\"\"\nx\"\"\""@2:5 #ID"b"@3:1`},
		{"a = '''\nx'''\nb = 1", `#ID"a"@1:1 #ST"'''\nx'''"@2:5 #ID"b"@3:1`},
		{"a = \"\"\"\nx\\\n  y\"\"\"\nb = 1", `#ID"a"@1:1 #ST"\"\"\"\nx\\\n  y\"\"\""@3:7 #ID"b"@4:1`},
		{"a = \"\"\nb = 1", `#ID"a"@1:1 #ST"\"\""@1:7 #ID"b"@2:1`},
		{"a = ''\nb = 1", `#ID"a"@1:1 #ST"''"@1:7 #ID"b"@2:1`},
		{"\"\" = 1", `#ST"\"\""@1:3`},
	} {
		if got := trace(c.src); got != c.want {
			t.Errorf("%q traces as\n  %s\nwant\n  %s", c.src, got, c.want)
		}
	}
}
