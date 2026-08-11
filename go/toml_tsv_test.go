// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// toml_tsv_test.go — cross-runtime conformance, driven by the shared
// `test/spec/*.tsv` fixtures at the repo root (see ../test/AGENTS.md).
//
// The fixture loader, the escape codec, the ERROR:<code> contract and the
// row loop all come from github.com/tabnas/support/go, whose TypeScript
// half ts/test/toml-tsv.test.ts uses to run the SAME files — so the two
// implementations cannot drift without one of them going red, and neither
// can the two loaders.
//
// What is left here is only what is specific to toml: how to build the
// parser, and flattening the result for comparison.

import (
	"errors"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
)

// TestSpec runs every fixture in the spec directory. Discovery is by
// listing, so adding a .tsv runs it in both runtimes without touching
// either runner — it used to have to be named in a list, once per runtime.
func TestSpec(t *testing.T) {
	dir, err := support.FindSpecDir("")
	if err != nil {
		t.Fatal(err)
	}

	support.Runner{
		// toml takes no per-row options, but a fresh parser per row keeps
		// one case's table state from reaching the next.
		Parse: func(input string) (any, error) {
			return MakeJsonic().Parse(input)
		},

		Normalize: normalizeNumbers,

		// One input is rejected by the two runtimes with DIFFERENT codes.
		// See divergentCode below: it is allowed by name, and only by name.
		MatchError: func(err error, want string, row *support.Row) bool {
			code := errorCode(err)
			if code == want {
				return true
			}
			// Only an input named in divergentCode, and only for the code
			// recorded against it. `ok` matters: without it a missing entry
			// reads as "" and would match any error this helper could not
			// read a code from — an allowance for everything.
			allowed, ok := divergentCode[row.Unesc(0)]
			return ok && allowed == code
		},
	}.Dir(t, dir)
}

// divergentCode records, per input, the code the GO parser answers where it
// differs from the TypeScript one the fixture names. TypeScript is
// canonical, so each entry is a defect in this port, not a licence.
//
// It surfaced the moment the shared runner started comparing codes at all:
// this suite used to accept any non-nil error, so an ERROR row asserted
// only that the input was rejected — not that it was rejected for the same
// reason. `"unterminated` is rejected by both, but TypeScript calls it
// `unexpected` and Go calls it `unterminated_string`.
//
// TestDivergentCodesAreStillDivergent below fails as soon as an entry stops
// being needed, so a fix cannot leave a stale allowance behind.
var divergentCode = map[string]string{
	`"unterminated`: "unterminated_string",
}

func TestDivergentCodesAreStillDivergent(t *testing.T) {
	for input, want := range divergentCode {
		_, err := MakeJsonic().Parse(input)
		if err == nil {
			t.Errorf("Parse(%q) no longer fails at all — remove its "+
				"divergentCode entry", input)
			continue
		}
		if got := errorCode(err); got != want {
			t.Errorf("Parse(%q) now answers %q, not the recorded %q — "+
				"update or remove its divergentCode entry", input, got, want)
		}
	}
}

// errorCode reads a parse error's code. The shared runner reads it by
// shape, because that module takes no dependencies; here the parser is
// already a dependency, so it can be read directly and an unreadable code
// is a loud "" rather than a quiet one.
func errorCode(err error) string {
	var te *tabnas.TabnasError
	if errors.As(err, &te) {
		return te.Code
	}
	return ""
}

// normalizeNumbers unwraps the insertion-ordered map and widens Go's
// integer types, so a result compares against the fixture's decoded JSON
// shape rather than against Go's idea of it.
func normalizeNumbers(v any) any {
	switch val := v.(type) {
	case int:
		return float64(val)
	case int64:
		return float64(val)
	case *jsonic.OrderedMap:
		return normalizeNumbers(val.Vals)
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
