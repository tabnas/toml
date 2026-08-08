// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	"math"
	"testing"
)

func TestSpecialFloats(t *testing.T) {
	cases := map[string]func(any) bool{
		"a = nan":   func(v any) bool { f, ok := v.(float64); return ok && math.IsNaN(f) },
		"a = +nan":  func(v any) bool { f, ok := v.(float64); return ok && math.IsNaN(f) },
		"a = -nan":  func(v any) bool { f, ok := v.(float64); return ok && math.IsNaN(f) },
		"a = inf":   func(v any) bool { f, ok := v.(float64); return ok && math.IsInf(f, 1) },
		"a = +inf":  func(v any) bool { f, ok := v.(float64); return ok && math.IsInf(f, 1) },
		"a = -inf":  func(v any) bool { f, ok := v.(float64); return ok && math.IsInf(f, -1) },
		"a = true":  func(v any) bool { b, ok := v.(bool); return ok && b },
		"a = false": func(v any) bool { b, ok := v.(bool); return ok && !b },
	}
	for input, check := range cases {
		res, err := Parse(input)
		if err != nil {
			t.Errorf("parse %q: %v", input, err)
			continue
		}
		m := resultMap(res)
		if !check(m["a"]) {
			t.Errorf("parse %q: got a=%v (%T)", input, m["a"], m["a"])
		}
	}
}

func TestTripleQuoted(t *testing.T) {
	cases := map[string]string{
		`a = """hello"""`:          "hello",
		`a = """"hello""""`:        `"hello"`,
		`a = '''hello'''`:          "hello",
		`a = """a` + "\n" + `b"""`: "a\nb",
	}
	for input, want := range cases {
		res, err := Parse(input)
		if err != nil {
			t.Errorf("parse %q: %v", input, err)
			continue
		}
		m := resultMap(res)
		if s, ok := m["a"].(string); !ok || s != want {
			t.Errorf("parse %q: want %q got %v (%T)", input, want, m["a"], m["a"])
		}
	}
}

func TestDatetime(t *testing.T) {
	cases := map[string]string{
		`a = 1979-05-27`:           "local-date",
		`a = 1979-05-27T07:32:00`:  "local-date-time",
		`a = 1979-05-27T07:32:00Z`: "offset-date-time",
		`a = 07:32:00`:             "local-time",
	}
	for input, wantKind := range cases {
		res, err := Parse(input)
		if err != nil {
			t.Errorf("parse %q: %v", input, err)
			continue
		}
		m := resultMap(res)
		dt, ok := m["a"].(*TomlTime)
		if !ok {
			t.Errorf("parse %q: got a=%v (%T), want *TomlTime", input, m["a"], m["a"])
			continue
		}
		if dt.Kind != wantKind {
			t.Errorf("parse %q: got kind=%q want %q", input, dt.Kind, wantKind)
		}
	}
}

// TestLeadingBOM pins the UTF-8 byte-order-mark rule: a TOML document may
// start with one and it is ignored, but a BOM anywhere else is an error.
// Covered by BurntSushi/toml-test valid/utf8-bom-01 and -02, which only run
// when that (optional) suite is installed — hence this local guard. The TS
// port has the matching test in ts/test/toml.test.ts.
func TestLeadingBOM(t *testing.T) {
	for _, src := range []string{"\uFEFFa = 1", "\uFEFF# c\na = 1"} {
		res, err := Parse(src)
		if err != nil {
			t.Errorf("parse %q: %v", src, err)
			continue
		}
		if v, ok := resultMap(res)["a"]; !ok || v != float64(1) {
			t.Errorf("parse %q: want a=1 got %v (%T)", src, v, v)
		}
	}

	// Not at the start: still an error.
	if _, err := Parse("a = 1\n\uFEFFb = 2"); err == nil {
		t.Error("BOM after the first character should not be accepted")
	}

	// Also via a directly built instance, not just the Parse wrapper.
	res, err := MakeJsonic().Parse("\uFEFFa = 1")
	if err != nil {
		t.Fatalf("MakeJsonic parse with BOM: %v", err)
	}
	if v, ok := resultMap(res)["a"]; !ok || v != float64(1) {
		t.Errorf("MakeJsonic parse with BOM: want a=1 got %v (%T)", v, v)
	}
}
