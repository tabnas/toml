// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// BurntSushi/toml-test conformance harness (Go side).
//
//	upstream: https://github.com/BurntSushi/toml-test
//	pinned:   9eef1b959e0449d41a31d4e4e0a839faee534b36
//
// The corpus is NOT committed (project rule: no vendored third-party
// test corpora). It is fetched by ../scripts/fetch-toml-test.sh into the
// gitignored ../ts/test/toml-test/. These tests invoke that script when
// the corpus is missing and FAIL LOUDLY if it still is not there.
//
// THIS SUITE MUST NEVER SKIP. It previously did — twice over:
//   - it looked for the corpus at ../test/toml-test, a path that has
//     never existed (the checkout lives at ../ts/test/toml-test), so
//     TestTomlValid t.Skipf'd on every run, everywhere, forever; and
//   - the invalid/ half of the suite was never loaded at all.
//
// Both halves are now asserted:
//
//	valid/   — must parse AND produce the correct VALUE
//	invalid/ — must be REJECTED with an error
//
// WHICH TOML VERSION IS JUDGED. README.md claims "A TOML parser" and
// links to https://toml.io, whose released specification is v1.0.0
// (v1.1.0 is unreleased). The ASSERTED corpus is therefore the suite's
// own tests/files-toml-1.0.0 manifest. The v1.1.0 and whole-corpus
// numbers are still measured and printed by TestTomlVersionReport so
// nothing is concealed.
//
// NO NAME-KEYED FIXUPS. The previous normaliser rewrote parsed values
// based on the FIXTURE NAME (an allFloat allow-list, a saturating int64
// hack for integer/long, a 3.0e14 case for float/underscore, a
// ten = 1e3 patch) and even re-read the fixture SOURCE to recover the
// sign of -0. Those made failing fixtures pass. They are gone. The only
// number rule now is the one an ordinary consumer must use:
// integer-looking -> integer, otherwise float.

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"

	jsonic "github.com/tabnas/jsonic/go"
)

const (
	suiteURL = "https://github.com/BurntSushi/toml-test"
	suitePin = "9eef1b959e0449d41a31d4e4e0a839faee534b36"
)

// suiteRoot is the toml-test checkout, relative to go/.
var suiteRoot = filepath.Join("..", "ts", "test", "toml-test")

var fetchScript = filepath.Join("..", "scripts", "fetch-toml-test.sh")

// ensureCorpus guarantees the conformance corpus is on disk. It never
// skips: if the corpus cannot be obtained the test FAILS, because a
// conformance test that silently does not run reports a green tick that
// is a lie.
func ensureCorpus(t *testing.T) string {
	t.Helper()

	present := func() bool {
		for _, half := range []string{"valid", "invalid"} {
			if _, err := os.Stat(filepath.Join(suiteRoot, "tests", half)); err != nil {
				return false
			}
		}
		return true
	}

	if present() {
		return suiteRoot
	}

	cmd := exec.Command("bash", fetchScript)
	cmd.Stdout = os.Stderr
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf(
			"BurntSushi/toml-test conformance corpus is MISSING and could not be fetched.\n"+
				"  suite:  %s @ %s\n"+
				"  expect: %s\n"+
				"  fix:    bash %s\n"+
				"This test deliberately FAILS rather than skipping.\n"+
				"  cause:  %v",
			suiteURL, suitePin, suiteRoot, fetchScript, err)
	}

	if !present() {
		t.Fatalf("toml-test corpus still absent after running %s (expected %s/tests/{valid,invalid})",
			fetchScript, suiteRoot)
	}

	return suiteRoot
}

// versionManifest reads the suite's own per-version file list.
func versionManifest(t *testing.T, root, version string) map[string]bool {
	t.Helper()
	path := filepath.Join(root, "tests", "files-toml-"+version)
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("toml-test version manifest missing: %s: %v", path, err)
	}
	out := map[string]bool{}
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasSuffix(line, ".toml") {
			out[line] = true
		}
	}
	return out
}

type conformanceFixture struct {
	rel  string // e.g. "valid/float/zero.toml"
	toml string
	json []byte
}

func collectFixtures(t *testing.T, root, half string) []conformanceFixture {
	t.Helper()
	base := filepath.Join(root, "tests", half)

	var out []conformanceFixture
	err := filepath.Walk(base, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() || !strings.HasSuffix(path, ".toml") {
			return err
		}
		rel, rerr := filepath.Rel(base, path)
		if rerr != nil {
			return rerr
		}
		f := conformanceFixture{rel: half + "/" + filepath.ToSlash(rel)}
		src, rerr := os.ReadFile(path)
		if rerr != nil {
			return rerr
		}
		f.toml = string(src)
		if half == "valid" {
			jsonSrc, jerr := os.ReadFile(strings.TrimSuffix(path, ".toml") + ".json")
			if jerr != nil {
				return jerr
			}
			f.json = jsonSrc
		}
		out = append(out, f)
		return nil
	})
	if err != nil {
		t.Fatalf("walk %s: %v", base, err)
	}

	sort.Slice(out, func(i, j int) bool { return out[i].rel < out[j].rel })
	return out
}

func filterManifest(fs []conformanceFixture, manifest map[string]bool) []conformanceFixture {
	var out []conformanceFixture
	for _, f := range fs {
		if manifest[f.rel] {
			out = append(out, f)
		}
	}
	return out
}

// --- value normalisation -------------------------------------------------

// normalizeForToml converts a Parse result into the {type, value} shape
// the toml-test fixtures use. Deliberately NAME-BLIND: the fixture name
// is never consulted.
func normalizeForToml(v any) any {
	switch x := v.(type) {
	case *jsonic.OrderedMap:
		return normalizeForToml(x.Vals)
	case map[string]any:
		out := make(map[string]any, len(x))
		for k, vv := range x {
			out[k] = normalizeForToml(vv)
		}
		return out
	case []any:
		out := make([]any, len(x))
		for i, vv := range x {
			out[i] = normalizeForToml(vv)
		}
		return out
	case string:
		return map[string]any{"type": "string", "value": x}
	case bool:
		return map[string]any{"type": "bool", "value": strconv.FormatBool(x)}
	case *TomlTime:
		return map[string]any{"type": tomlTimeJSONType(x.Kind), "value": canonDatetime(x.Src)}
	case float64:
		return formatNumber(x)
	case float32:
		return formatNumber(float64(x))
	case int:
		return formatNumber(float64(x))
	case int64:
		return formatNumber(float64(x))
	case nil:
		return nil
	}
	return v
}

// formatNumber applies the same name-blind rule as the TS harness:
// integer-looking -> integer, otherwise float.
func formatNumber(v float64) any {
	if math.IsNaN(v) {
		return map[string]any{"type": "float", "value": "nan"}
	}
	if math.IsInf(v, 1) {
		return map[string]any{"type": "float", "value": "inf"}
	}
	if math.IsInf(v, -1) {
		return map[string]any{"type": "float", "value": "-inf"}
	}

	// TS forms the integer string with `'' + v`, and in JS `'' + (-0)` is
	// "0". Match that so the two runtimes make the same (lossy) choice.
	if v == 0 {
		v = 0
	}
	asInt := strconv.FormatFloat(v, 'f', -1, 64)
	if intishRe.MatchString(asInt) {
		return map[string]any{"type": "integer", "value": asInt}
	}
	return map[string]any{"type": "float", "value": goFloatString(v)}
}

// goFloatString formats a float64 the way the TS goFloat() helper does:
// pick the shorter of decimal vs. scientific (ties go to decimal).
func goFloatString(v float64) string {
	if v == 0 {
		if math.Signbit(v) {
			return "-0"
		}
		return "0"
	}
	dec := strconv.FormatFloat(v, 'f', -1, 64)
	sci := ensureExpSign(strconv.FormatFloat(v, 'e', -1, 64))
	if len(dec) <= len(sci) {
		return dec
	}
	return sci
}

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

// canonDatetime canonicalises a TOML datetime string. Applied to BOTH
// the parsed value and the expected fixture value, so it can only
// collapse semantically identical spellings (separator case, a space
// used as the date/time separator, trailing zeros in the fraction, an
// omitted :SS). It cannot mask a wrong value. Mirrors the TS
// canonDatetime().
func canonDatetime(s string) string {
	v := strings.TrimSpace(s)
	v = dateSepRe.ReplaceAllString(v, "${1}T")
	v = trailingZRe.ReplaceAllString(v, "Z")
	v = hourMinRe.ReplaceAllString(v, "${1}${2}:00")
	v = fracZerosRe.ReplaceAllStringFunc(v, func(m string) string {
		d := strings.TrimRight(strings.TrimPrefix(m, "."), "0")
		if d == "" {
			return ""
		}
		return "." + d
	})
	return v
}

var (
	intishRe    = regexp.MustCompile(`^-?\d+$`)
	dateSepRe   = regexp.MustCompile(`^(\d{4}-\d\d-\d\d)[ tT]`)
	trailingZRe = regexp.MustCompile(`[zZ]$`)
	// HH:MM not followed by :SS / fraction / offset -> pad seconds.
	hourMinRe = regexp.MustCompile(`(T|^)(\d\d:\d\d)($|[.Z+-])`)
	// Fractional seconds; trailing zeros trimmed (empty fraction dropped).
	fracZerosRe = regexp.MustCompile(`\.\d+`)
)

// canonExpected canonicalises the expected fixture tree the same way for
// datetimes, so the comparison is symmetric.
func canonExpected(v any) any {
	switch x := v.(type) {
	case map[string]any:
		if ty, ok := x["type"].(string); ok {
			if val, ok2 := x["value"].(string); ok2 {
				if strings.HasPrefix(ty, "datetime") ||
					strings.HasPrefix(ty, "date-") ||
					strings.HasPrefix(ty, "time-") {
					return map[string]any{"type": ty, "value": canonDatetime(val)}
				}
				return map[string]any{"type": ty, "value": val}
			}
		}
		out := make(map[string]any, len(x))
		for k, vv := range x {
			out[k] = canonExpected(vv)
		}
		return out
	case []any:
		out := make([]any, len(x))
		for i, vv := range x {
			out[i] = canonExpected(vv)
		}
		return out
	}
	return v
}

func deepEqual(a, b any) bool {
	switch av := a.(type) {
	case map[string]any:
		bv, ok := b.(map[string]any)
		if !ok || len(av) != len(bv) {
			return false
		}
		for k, vv := range av {
			other, present := bv[k]
			if !present || !deepEqual(vv, other) {
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

// --- runners -------------------------------------------------------------

type conformanceReport struct {
	total int
	pass  int
	fail  int
	fails []string
	// Invalid documents rejected by a PANIC rather than a diagnosed
	// parse error. They still count as rejected, but a panic is not a
	// conformant rejection, so the distinction is reported.
	crashRejects []string
}

func (r conformanceReport) summary(label string) string {
	pct := 0.0
	if r.total > 0 {
		pct = 100 * float64(r.pass) / float64(r.total)
	}
	crash := ""
	if len(r.crashRejects) > 0 {
		crash = fmt.Sprintf("  (of which %d rejected by INTERNAL PANIC, not a diagnosed parse error)",
			len(r.crashRejects))
	}
	return fmt.Sprintf("%s: %d/%d (%.1f%%)  failures=%d%s",
		label, r.pass, r.total, pct, r.fail, crash)
}

// safeParse calls Parse, converting a panic into an error so one bad
// fixture cannot abort the whole measurement. `panicked` distinguishes
// an internal crash from a diagnosed parse error.
func safeParse(src string) (out any, err error, panicked bool) {
	defer func() {
		if rec := recover(); rec != nil {
			out = nil
			err = fmt.Errorf("PANIC: %v", rec)
			panicked = true
		}
	}()
	out, err = Parse(src)
	return
}

func runValid(fixtures []conformanceFixture) conformanceReport {
	r := conformanceReport{total: len(fixtures)}
	for _, f := range fixtures {
		out, err, _ := safeParse(f.toml)
		if err != nil {
			r.fail++
			r.fails = append(r.fails, f.rel+"  PARSE ERROR: "+firstLine(err.Error()))
			continue
		}

		var expected any
		if err := json.Unmarshal(f.json, &expected); err != nil {
			r.fail++
			r.fails = append(r.fails, f.rel+"  BAD EXPECTED JSON: "+err.Error())
			continue
		}

		got := normalizeForToml(out)
		want := canonExpected(expected)

		if !deepEqual(got, want) {
			r.fail++
			gotJSON, _ := json.Marshal(got)
			wantJSON, _ := json.Marshal(want)
			r.fails = append(r.fails, fmt.Sprintf("%s  WRONG VALUE\n      got:  %s\n      want: %s",
				f.rel, string(gotJSON), string(wantJSON)))
			continue
		}
		r.pass++
	}
	return r
}

func runInvalid(fixtures []conformanceFixture) conformanceReport {
	r := conformanceReport{total: len(fixtures)}
	for _, f := range fixtures {
		out, err, panicked := safeParse(f.toml)
		if err != nil {
			r.pass++
			if panicked {
				r.crashRejects = append(r.crashRejects, f.rel+": "+firstLine(err.Error()))
			}
			continue
		}
		r.fail++
		gotJSON, _ := json.Marshal(normalizeForToml(out))
		r.fails = append(r.fails, fmt.Sprintf("%s  WRONGLY ACCEPTED\n      src:  %q\n      got:  %s",
			f.rel, f.toml, string(gotJSON)))
	}
	return r
}

// maxFail bounds how many individual failures are printed. Raise with
// TOML_CONFORMANCE_MAX_FAIL=0 (unlimited) when triaging; the pass/fail
// counts are always exact.
func maxFail(def int) int {
	if raw := os.Getenv("TOML_CONFORMANCE_MAX_FAIL"); raw != "" {
		if n, err := strconv.Atoi(raw); err == nil && n >= 0 {
			if n == 0 {
				return int(^uint(0) >> 1)
			}
			return n
		}
	}
	return def
}

func reportFailures(t *testing.T, label string, r conformanceReport, limit int) {
	t.Helper()
	limit = maxFail(limit)
	t.Logf("%s", r.summary(label))
	show := r.fails
	if len(show) > limit {
		show = show[:limit]
	}
	for _, msg := range show {
		t.Logf("  FAIL %s", msg)
	}
	if len(r.fails) > limit {
		t.Logf("  ...and %d more failures", len(r.fails)-limit)
	}
	showCrash := r.crashRejects
	if len(showCrash) > limit {
		showCrash = showCrash[:limit]
	}
	for _, msg := range showCrash {
		t.Logf("  CRASH-REJECT %s", msg)
	}
	if len(r.crashRejects) > limit {
		t.Logf("  ...and %d more crash-rejections", len(r.crashRejects)-limit)
	}
}

// --- tests ---------------------------------------------------------------

// TestTomlValid: every TOML 1.0.0 valid document must parse to the
// expected value.
func TestTomlValid(t *testing.T) {
	root := ensureCorpus(t)
	manifest := versionManifest(t, root, "1.0.0")
	fixtures := filterManifest(collectFixtures(t, root, "valid"), manifest)

	if len(fixtures) == 0 {
		t.Fatalf("toml-test valid/ corpus for TOML 1.0.0 is empty — corpus is broken")
	}

	r := runValid(fixtures)
	reportFailures(t, "toml-valid (TOML 1.0.0)", r, 40)

	if r.fail > 0 {
		t.Errorf("%d of %d TOML 1.0.0 valid documents did not parse to the expected value (suite %s @ %s)",
			r.fail, r.total, suiteURL, suitePin)
	}
}

// TestTomlInvalid: every TOML 1.0.0 invalid document must be rejected.
func TestTomlInvalid(t *testing.T) {
	root := ensureCorpus(t)
	manifest := versionManifest(t, root, "1.0.0")
	fixtures := filterManifest(collectFixtures(t, root, "invalid"), manifest)

	if len(fixtures) == 0 {
		t.Fatalf("toml-test invalid/ corpus for TOML 1.0.0 is empty — corpus is broken")
	}

	r := runInvalid(fixtures)
	reportFailures(t, "toml-invalid (TOML 1.0.0)", r, 40)

	if r.fail > 0 {
		t.Errorf("%d of %d TOML 1.0.0 INVALID documents were wrongly ACCEPTED (suite %s @ %s)",
			r.fail, r.total, suiteURL, suitePin)
	}
}

// TestTomlVersionReport measures the unreleased TOML 1.1.0 draft and the
// whole corpus. Reported, not asserted: the package does not claim
// v1.1.0. It exists so the full corpus is never hidden.
func TestTomlVersionReport(t *testing.T) {
	root := ensureCorpus(t)

	allValid := collectFixtures(t, root, "valid")
	allInvalid := collectFixtures(t, root, "invalid")

	m110 := versionManifest(t, root, "1.1.0")
	t.Logf("--- TOML 1.1.0 draft (reported, NOT asserted: not claimed) ---")
	t.Logf("  %s", runValid(filterManifest(allValid, m110)).summary("toml-1.1.0-valid"))
	t.Logf("  %s", runInvalid(filterManifest(allInvalid, m110)).summary("toml-1.1.0-invalid"))

	t.Logf("--- Whole corpus (reported, NOT asserted) ---")
	t.Logf("  %s", runValid(allValid).summary("all-valid"))
	t.Logf("  %s", runInvalid(allInvalid).summary("all-invalid"))
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i]
	}
	return s
}
