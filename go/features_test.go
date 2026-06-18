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
		m := res.(map[string]any)
		if !check(m["a"]) {
			t.Errorf("parse %q: got a=%v (%T)", input, m["a"], m["a"])
		}
	}
}

func TestTripleQuoted(t *testing.T) {
	cases := map[string]string{
		`a = """hello"""`:        "hello",
		`a = """"hello""""`:      `"hello"`,
		`a = '''hello'''`:        "hello",
		`a = """a` + "\n" + `b"""`: "a\nb",
	}
	for input, want := range cases {
		res, err := Parse(input)
		if err != nil {
			t.Errorf("parse %q: %v", input, err)
			continue
		}
		m := res.(map[string]any)
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
		m := res.(map[string]any)
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
