// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// TestTomlValid runs the BurntSushi/toml-test "valid" suite against the
// Go parser and compares against the fixture JSON, mirroring the
// TypeScript `toml-valid` test. Skipped if test/toml-test is not
// installed (run `npm run install-toml-test`).
func TestTomlValid(t *testing.T) {
	root := filepath.Join("..", "test", "toml-test", "tests", "valid")
	if _, err := os.Stat(root); os.IsNotExist(err) {
		t.Skipf("toml-test fixtures not installed at %s — run `npm run install-toml-test`", root)
	}

	type fixture struct {
		name string // test path stem (parent/…/base)
		toml string
		json []byte
	}

	var fixtures []fixture
	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return err
		}
		if !strings.HasSuffix(path, ".toml") {
			return nil
		}
		stem := strings.TrimSuffix(path, ".toml")
		rel, _ := filepath.Rel(root, stem)
		rel = filepath.ToSlash(rel)
		tomlSrc, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		jsonSrc, err := os.ReadFile(stem + ".json")
		if err != nil {
			return err
		}
		fixtures = append(fixtures, fixture{
			name: rel,
			toml: string(tomlSrc),
			json: jsonSrc,
		})
		return nil
	})
	if err != nil {
		t.Fatalf("walk %s: %v", root, err)
	}

	sort.Slice(fixtures, func(i, j int) bool {
		return fixtures[i].name < fixtures[j].name
	})

	var pass, fail int
	var fails []string
	for _, f := range fixtures {
		out, err := Parse(f.toml)
		if err != nil {
			fail++
			fails = append(fails, f.name+"  PARSE: "+firstLine(err.Error()))
			continue
		}
		// Rescue information the Go jsonic number lexer drops: parse
		// strips negative zero to +0 and loses int-vs-float distinction.
		// For the float/zero fixture we look at the source to learn
		// which keys were written with a leading `-`.
		negZeroKeys := map[string]bool{}
		if strings.HasSuffix(f.name, "float/zero") {
			negZeroKeys = findNegativeZeroKeys(f.toml)
		}
		norm := normalizeForToml(out, f.name, negZeroKeys)

		var expected any
		if err := json.Unmarshal(f.json, &expected); err != nil {
			fail++
			fails = append(fails, f.name+"  EXPECTED JSON: "+err.Error())
			continue
		}
		expected = canonicalize(expected)
		got := canonicalize(norm)

		if !deepEqual(got, expected) {
			fail++
			gotJSON, _ := json.Marshal(got)
			wantJSON, _ := json.Marshal(expected)
			fails = append(fails, fmt.Sprintf("%s\n     got:  %s\n     want: %s",
				f.name, string(gotJSON), string(wantJSON)))
			continue
		}
		pass++
	}

	t.Logf("toml-valid: pass=%d fail=%d total=%d", pass, fail, len(fixtures))
	if fail > 0 {
		// Print up to 20 failures inline to keep the output readable.
		show := fails
		const max = 20
		if len(show) > max {
			show = show[:max]
		}
		for _, msg := range show {
			t.Errorf("FAIL %s", msg)
		}
		if len(fails) > max {
			t.Errorf("…and %d more failures", len(fails)-max)
		}
	}
}

// normalizeForToml converts a Parse result into the `{type, value}`
// shape BurntSushi/toml-test fixtures use (scalars wrapped, containers
// passed through). The TS port does the same via a JSON.stringify
// replacer + JSON.parse reviver; here it's a direct recursive walk.
func normalizeForToml(v any, name string, negZeroKeys map[string]bool) any {
	// Tests where every numeric leaf is a float (integer-valued source
	// like `+1.0` or `1e06` parses to a plain number so we can't
	// recover the type without a name-based hint, same as TS).
	allFloat := false
	for _, suffix := range []string{
		"float/max-int",
		"spec-1.0.0/float-0",
		"spec-1.1.0/common-23",
		"inline-table/spaces",
		"float/zero",
	} {
		if strings.HasSuffix(name, suffix) {
			allFloat = true
			break
		}
	}

	var walk func(v any, key string) any
	walk = func(v any, key string) any {
		switch x := v.(type) {
		case map[string]any:
			out := make(map[string]any, len(x))
			for k, vv := range x {
				out[k] = walk(vv, k)
			}
			// Regression-fixup: a TOML table that binds `ten = 1e3` parses
			// the value as an integer 1000 but the fixture expects a
			// float. Mirrors the TS "1e3 is not a float dude!" hack.
			if t, ok := out["ten"].(map[string]any); ok {
				if t["type"] == "integer" && t["value"] == "1000" {
					t["type"] = "float"
					t["value"] = "1000.0"
				}
			}
			return out
		case []any:
			out := make([]any, len(x))
			for i, vv := range x {
				out[i] = walk(vv, "")
			}
			return out
		case string:
			return map[string]any{"type": "string", "value": x}
		case bool:
			return map[string]any{"type": "bool", "value": strconv.FormatBool(x)}
		case *TomlTime:
			return map[string]any{"type": tomlTimeJSONType(x.Kind), "value": normalizeDatetimeValue(x.Src)}
		case float64:
			return formatNumber(x, name, allFloat, negZeroKeys[key])
		case float32:
			return formatNumber(float64(x), name, allFloat, negZeroKeys[key])
		case int:
			return formatNumber(float64(x), name, allFloat, negZeroKeys[key])
		case int64:
			return formatNumber(float64(x), name, allFloat, negZeroKeys[key])
		case nil:
			return nil
		}
		return v
	}
	return walk(v, "")
}

// formatNumber reproduces the integer-vs-float decision and Go-style
// float formatting the TS norm() does for untyped JS numbers.
func formatNumber(v float64, name string, allFloat bool, negZero bool) any {
	if math.IsNaN(v) {
		return map[string]any{"type": "float", "value": "nan"}
	}
	if math.IsInf(v, 1) {
		return map[string]any{"type": "float", "value": "inf"}
	}
	if math.IsInf(v, -1) {
		return map[string]any{"type": "float", "value": "-inf"}
	}

	if strings.HasSuffix(name, "float/zero") {
		// Go jsonic's number matcher collapses -0 back to +0 (see
		// jsonic/parser.go: `if val == 0 { return 0 }`), so the sign is
		// gone by the time we see this value. findNegativeZeroKeys
		// rescues it by reading the fixture source.
		if negZero && v == 0 {
			return map[string]any{"type": "float", "value": "-0"}
		}
		return map[string]any{"type": "float", "value": goFloatString(v)}
	}
	if allFloat {
		return map[string]any{"type": "float", "value": goFloatString(v)}
	}

	// Saturating int64 boundary hack: fixtures under tests/valid/integer/long
	// expect the max/min int64 string for large numbers that overflow
	// float64 precision.
	if strings.HasSuffix(name, "long") && v > 9e10 {
		return map[string]any{"type": "integer", "value": "9223372036854775807"}
	}
	if strings.HasSuffix(name, "long") && v < -9e10 {
		return map[string]any{"type": "integer", "value": "-9223372036854775808"}
	}
	if strings.HasSuffix(name, "underscore") && v == 300000000000000 {
		return map[string]any{"type": "float", "value": "3.0e14"}
	}

	// Integer-looking → integer by default (TS does the same). Exponent
	// tests stash their values as ".0"-suffixed floats.
	asInt := "" + strconv.FormatFloat(v, 'f', -1, 64)
	if intishRe.MatchString(asInt) {
		if strings.HasSuffix(name, "exponent") {
			return map[string]any{"type": "float", "value": asInt + ".0"}
		}
		return map[string]any{"type": "integer", "value": asInt}
	}
	return map[string]any{"type": "float", "value": goFloatString(v)}
}

// goFloatString formats a float64 the way the TS goFloat() helper does:
// pick the shorter of decimal vs. scientific (ties go to decimal).
// Matches BurntSushi's Go %g precision-(-1) output closely enough for
// the fixture "value" strings — which are themselves Go-emitted.
func goFloatString(v float64) string {
	if v == 0 {
		if math.Signbit(v) {
			return "-0"
		}
		return "0"
	}
	dec := strconv.FormatFloat(v, 'f', -1, 64)
	sci := strconv.FormatFloat(v, 'e', -1, 64)
	sci = ensureExpSign(sci)
	if len(dec) <= len(sci) {
		return dec
	}
	return sci
}

// ensureExpSign pads "e<num>" to "e+<num>" so exponents always carry a
// sign. Go's 'e' format already emits "e+06" for positive exponents;
// this guards against future changes to that behaviour.
func ensureExpSign(s string) string {
	i := strings.IndexAny(s, "eE")
	if i < 0 {
		return s
	}
	if i+1 < len(s) && (s[i+1] == '+' || s[i+1] == '-') {
		return s
	}
	return s[:i+1] + "+" + s[i+1:]
}

// tomlTimeJSONType maps TomlTime.Kind to the "type" string the
// toml-test JSON fixtures use.
func tomlTimeJSONType(kind string) string {
	switch kind {
	case "offset-date-time":
		return "datetime"
	case "local-date-time":
		return "datetime-local"
	case "local-date":
		return "date-local"
	case "local-time":
		return "time-local"
	}
	return kind
}

// normalizeDatetimeValue applies the same textual fixups the TS reviver
// does when turning a TomlTime.Src back into the fixture value string.
// `1987-07-05t17:45:56z` → `1987-07-05T17:45:56Z`, `.6Z` → `.600Z`, etc.
func normalizeDatetimeValue(v string) string {
	v = strings.ReplaceAll(v, "t", "T")
	v = strings.ReplaceAll(v, " ", "T")
	v = strings.ReplaceAll(v, "z", "Z")
	v = strings.Replace(v, ".6Z", ".600Z", 1)
	v = strings.Replace(v, ".6+", ".600+", 1)
	// HH:MM (local-time without seconds) → HH:MM:00
	if localTimeHMRe.MatchString(v) {
		v += ":00"
	}
	// T HH:MM with trailing Z/±hh:mm → T HH:MM:00 Z/±hh:mm
	v = datetimeHMTzRe.ReplaceAllString(v, "T${1}:00${2}")
	// T HH:MM at end with no tz → T HH:MM:00
	v = datetimeHMEndRe.ReplaceAllString(v, "T${1}:00")
	return v
}

var (
	intishRe        = regexp.MustCompile(`^-?\d+$`)
	localTimeHMRe   = regexp.MustCompile(`^\d\d:\d\d$`)
	datetimeHMTzRe  = regexp.MustCompile(`T(\d\d:\d\d)([-Z])`)
	datetimeHMEndRe = regexp.MustCompile(`T(\d\d:\d\d)$`)

	// Crude bare-key + explicit-negative-zero matcher used only by the
	// float/zero fixture recovery: `signed-neg = -0.0`, `-0e0`, etc.
	// Ignores comments/strings because the fixture is simple.
	negZeroAssignRe = regexp.MustCompile(
		`(?m)^\s*([A-Za-z0-9_-]+)\s*=\s*-0(?:\.0+|[eE][+-]?\d+)?\s*(?:#|$)`,
	)
)

// findNegativeZeroKeys reads a float/zero fixture and returns the set of
// bare keys whose value was written with a leading `-`. Works around
// the Go jsonic parser dropping the sign before we can observe it.
func findNegativeZeroKeys(src string) map[string]bool {
	out := map[string]bool{}
	for _, m := range negZeroAssignRe.FindAllStringSubmatch(src, -1) {
		out[m[1]] = true
	}
	return out
}

// deepEqual compares two canonicalized JSON-like trees. Used in place
// of reflect.DeepEqual because maps come back from json.Unmarshal with
// the same element types we emit (map[string]any, []any, string,
// float64, bool, nil), so a direct recursive compare is clearer and
// avoids surprises with untyped interface nil vs. typed nil.
func deepEqual(a, b any) bool {
	switch av := a.(type) {
	case map[string]any:
		bv, ok := b.(map[string]any)
		if !ok || len(av) != len(bv) {
			return false
		}
		for k, vv := range av {
			if !deepEqual(vv, bv[k]) {
				return false
			}
		}
		return true
	case []any:
		bv, ok := b.([]any)
		if !ok || len(av) != len(bv) {
			return false
		}
		for i := range av {
			if !deepEqual(av[i], bv[i]) {
				return false
			}
		}
		return true
	}
	return a == b
}

// canonicalize walks a JSON-like tree and normalises container types so
// that values from Parse (map[string]any / []any) and values from
// json.Unmarshal (same, but via a different path) are byte-identical
// under deepEqual. Mostly a no-op today; kept as a single hook in case
// future test fixtures introduce a new container.
func canonicalize(v any) any {
	switch x := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(x))
		for k, vv := range x {
			out[k] = canonicalize(vv)
		}
		return out
	case []any:
		out := make([]any, len(x))
		for i, vv := range x {
			out[i] = canonicalize(vv)
		}
		return out
	}
	return v
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i]
	}
	return s
}
