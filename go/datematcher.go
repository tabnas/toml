// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	"regexp"

	tabnas "github.com/tabnas/parser/go"
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
func isKeyContext(idTin tabnas.Tin, rule *tabnas.Rule) bool {
	if rule == nil || rule.Spec == nil {
		return false
	}
	alts := rule.Spec.OpenAlts()
	if rule.State == tabnas.CLOSE {
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
// by @isodate-val / @localtime-val, and `inRange` rejects a value whose
// shape matched but whose components cannot denote a real instant.
func makeDateMatcher(
	idTin tabnas.Tin,
	re *regexp.Regexp,
	toVal func([]string) any,
	inRange func(string) bool,
) tabnas.LexMatcher {
	return func(lex *tabnas.Lex, rule *tabnas.Rule) *tabnas.Token {
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
		var tkn *tabnas.Token
		if isKeyContext(idTin, rule) {
			// A bare key that merely looks like a date is not a date, so its
			// components are not required to be in range: `2006-01-32 = 1`
			// defines a key, and rejecting it here would be a range check
			// applied to a name.
			tkn = lex.Token("#ID", idTin, msrc, msrc)
		} else {
			if !inRange(msrc) {
				return lex.Bad("invalid_datetime")
			}
			tkn = lex.Token("#VL", tabnas.TinVL, toVal(m), msrc)
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
func registerDateMatchers(j *tabnas.Tabnas) {
	idTin := j.Token("#ID")
	isodate := makeDateMatcher(idTin, isodateRe, isodateVal, isodateInRange)
	localtime := makeDateMatcher(
		idTin, localtimeRe, localtimeVal, localtimeInRange)

	j.SetOptions(tabnas.Options{
		Lex: &tabnas.LexOptions{
			Match: map[string]*tabnas.MatchSpec{
				"tomlisodate": {
					Order: 950000,
					Make: func(_ *tabnas.LexConfig, _ *tabnas.Options) tabnas.LexMatcher {
						return isodate
					},
				},
				"tomllocaltime": {
					Order: 950001,
					Make: func(_ *tabnas.LexConfig, _ *tabnas.Options) tabnas.LexMatcher {
						return localtime
					},
				},
			},
		},
	})
}
