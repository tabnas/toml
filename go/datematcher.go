// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package toml

import (
	"regexp"

	jsonic "github.com/tabnas/jsonic/go"
)

// Date / time value regexps — the same shapes the grammar's
// regex-based isodate/localtime value matchers recognise.
var (
	isodateRe = regexp.MustCompile(
		`^\d\d\d\d-\d\d-\d\d([Tt ]\d\d:\d\d(:\d\d(\.\d+)?)?([Zz]|[-+]\d\d:\d\d)?)?`,
	)
	localtimeRe = regexp.MustCompile(
		`^\d\d:\d\d(:\d\d(\.\d+)?)?`,
	)
)

// isKeyContext reports whether the current rule position accepts a bare
// key (the #ID token). Bare-key-shaped dates like `2001-02-03 = 1` or
// `[2002-01-02]` or `a.2001-02-08 = 7` must fall through to #ID lexing
// instead of being swallowed by the regex date-value matcher that would
// otherwise claim them unconditionally.
//
// Value-producing rules (val, list, elem) never list #ID in their alts;
// key-accepting rules (toml, map, dive, pair, table) do. Custom matchers
// aren't told which tI they're at, so we scan every alt's S[*] to stay
// consistent with the TS port's tcol-scan heuristic.
func isKeyContext(idTin jsonic.Tin, rule *jsonic.Rule) bool {
	if rule == nil || rule.Spec == nil {
		return false
	}
	alts := rule.Spec.OpenAlts()
	if rule.State == jsonic.CLOSE {
		alts = rule.Spec.CloseAlts()
	}
	for _, alt := range alts {
		for _, pos := range alt.S {
			for _, tin := range pos {
				if tin == idTin {
					return true
				}
			}
		}
	}
	return false
}

// makeDateMatcher returns a LexMatcher that recognises a date/time shape
// and either (a) emits a #ID token so a bare-key position can claim the
// text, or (b) emits a #VL token carrying a *TomlTime for value positions.
// The `toVal` adapter turns regex match groups into the TomlTime returned
// by @isodate-val / @localtime-val.
func makeDateMatcher(
	idTin jsonic.Tin,
	re *regexp.Regexp,
	toVal func([]string) any,
) jsonic.LexMatcher {
	return func(lex *jsonic.Lex, rule *jsonic.Rule) *jsonic.Token {
		pnt := lex.Cursor()
		if pnt.SI >= len(lex.Src) {
			return nil
		}
		m := re.FindStringSubmatch(lex.Src[pnt.SI:])
		if m == nil {
			return nil
		}
		msrc := m[0]
		mlen := len(msrc)
		var tkn *jsonic.Token
		if isKeyContext(idTin, rule) {
			tkn = lex.Token("#ID", idTin, msrc, msrc)
		} else {
			tkn = lex.Token("#VL", jsonic.TinVL, toVal(m), msrc)
		}
		pnt.SI += mlen
		pnt.CI += mlen
		return tkn
	}
}

// registerDateMatchers installs context-aware date/time matchers at a
// priority below matchMatch (1e6) so they pre-empt the grammar's
// regex-based isodate / localtime value matchers. Those regex entries
// remain in the config as dead code — this matcher always consumes any
// text they would have matched first.
func registerDateMatchers(j *jsonic.Jsonic) {
	idTin := j.Token("#ID")
	isodate := makeDateMatcher(idTin, isodateRe, isodateVal)
	localtime := makeDateMatcher(idTin, localtimeRe, localtimeVal)

	j.SetOptions(jsonic.Options{
		Lex: &jsonic.LexOptions{
			Match: map[string]*jsonic.MatchSpec{
				"tomlisodate": {
					Order: 950000,
					Make: func(_ *jsonic.LexConfig, _ *jsonic.Options) jsonic.LexMatcher {
						return isodate
					},
				},
				"tomllocaltime": {
					Order: 950001,
					Make: func(_ *jsonic.LexConfig, _ *jsonic.Options) jsonic.LexMatcher {
						return localtime
					},
				},
			},
		},
	})
}
