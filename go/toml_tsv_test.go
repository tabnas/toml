// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package toml

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

// Subset of TSV spec files that the current Go skeleton passes.
var tsvSubset = []string{
	"happy",
	"basic-values",
	"integers",
	"floats",
	"strings",
	"arrays",
	"dotted-keys",
	"inline-tables",
	"quoted-keys",
	"comments",
	"tables",
	"array-of-tables",
	"whitespace",
	"mixed",
	"errors",
}

func unescape(s string) string {
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		if s[i] == '\\' && i+1 < len(s) {
			switch s[i+1] {
			case 'n':
				b.WriteByte('\n')
				i++
				continue
			case 'r':
				if i+2 < len(s) && s[i+2] == '\\' && i+3 < len(s) && s[i+3] == 'n' {
					b.WriteString("\r\n")
					i += 3
					continue
				}
				b.WriteByte('\r')
				i++
				continue
			case 't':
				b.WriteByte('\t')
				i++
				continue
			}
		}
		b.WriteByte(s[i])
	}
	return b.String()
}

// normalizeNumbers walks a parse result and converts int → float64 so
// comparisons against JSON-parsed expected values succeed.
func normalizeNumbers(v any) any {
	switch val := v.(type) {
	case int:
		return float64(val)
	case int64:
		return float64(val)
	case map[string]any:
		out := make(map[string]any, len(val))
		for k, vv := range val {
			out[k] = normalizeNumbers(vv)
		}
		return out
	case []any:
		out := make([]any, len(val))
		for i, vv := range val {
			out[i] = normalizeNumbers(vv)
		}
		return out
	}
	return v
}

type tsvRow struct {
	input    string
	expected string
	rowNum   int
}

func loadTSV(t *testing.T, name string) []tsvRow {
	t.Helper()
	path := filepath.Join("..", "test", "spec", name+".tsv")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	lines := strings.Split(string(data), "\n")
	rows := make([]tsvRow, 0, len(lines))
	for i, line := range lines {
		if line == "" {
			continue
		}
		if i == 0 {
			continue
		}
		cols := strings.SplitN(line, "\t", 2)
		if len(cols) < 2 {
			continue
		}
		rows = append(rows, tsvRow{
			input:    unescape(cols[0]),
			expected: unescape(cols[1]),
			rowNum:   i + 1,
		})
	}
	return rows
}

func runTSV(t *testing.T, name string) {
	t.Helper()
	j := MakeJsonic()
	for _, r := range loadTSV(t, name) {
		if strings.HasPrefix(r.expected, "ERROR:") {
			if _, err := j.Parse(r.input); err == nil {
				t.Errorf("%s.tsv row %d: expected error for input=%q, got success",
					name, r.rowNum, r.input)
			}
			continue
		}

		actual, err := j.Parse(r.input)
		if err != nil {
			t.Errorf("%s.tsv row %d: parse failed: input=%q err=%v",
				name, r.rowNum, r.input, err)
			continue
		}

		var expected any
		if err := json.Unmarshal([]byte(r.expected), &expected); err != nil {
			t.Errorf("%s.tsv row %d: bad expected JSON: %v", name, r.rowNum, err)
			continue
		}

		got := normalizeNumbers(actual)
		if !reflect.DeepEqual(got, expected) {
			t.Errorf("%s.tsv row %d: input=%q\n  want %s\n  got  %s",
				name, r.rowNum, r.input,
				mustJSON(expected), mustJSON(got))
		}
	}
}

func mustJSON(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		return fmt.Sprintf("<marshal err: %v>", err)
	}
	return string(b)
}

// Ensure our test runner survives the skeleton's known parser.
var _ = jsonic.Version // keep the import useful

func TestTSV(t *testing.T) {
	for _, name := range tsvSubset {
		name := name
		t.Run(name, func(t *testing.T) {
			runTSV(t, name)
		})
	}
}
