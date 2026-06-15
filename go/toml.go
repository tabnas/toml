// Copyright (c) 2021-2026 Richard Rodger, MIT License

// Package toml is a Go port of @tabnas/toml, a Jsonic plugin that
// parses TOML syntax into Go maps. See README for the currently
// supported subset.
package toml

import (
	"fmt"
	"math"

	jsonic "github.com/tabnas/jsonic/go"
)

const Version = "0.1.2"

// --- BEGIN EMBEDDED toml-grammar.jsonic ---
const grammarText = `
# TOML Grammar Definition
# Parsed by a standard Jsonic instance and passed to jsonic.grammar()
# Function references (@ prefixed) are resolved against the refs map.
# Regex references (@/pattern/flags) are resolved to RegExp instances.

{
  options: rule: { start: toml exclude: jsonic }
  options: lex: {
    emptyResult: {}
    match: string: make: '@make-toml-string-matcher'
  }
  options: fixed: token: { '#CL': '=' '#DOT': '.' }
  options: match: {
    token: { '#ID': '@/^[a-zA-Z0-9_-]+/' }
    value: {
      isodate: {
        match: '@/^\\d\\d\\d\\d-\\d\\d-\\d\\d([Tt ]\\d\\d:\\d\\d(:\\d\\d(\\.\\d+)?)?([Zz]|[-+]\\d\\d:\\d\\d)?)?/'
        val: '@isodate-val'
      }
      localtime: {
        match: '@/^\\d\\d:\\d\\d(:\\d\\d(\\.\\d+)?)?/'
        val: '@localtime-val'
      }
    }
  }
  options: tokenSet: {
    KEY: ['#ST' '#ID' null null]
  }
  options: comment: def: { slash: null multi: null }

  rule: toml: open: [
    { s: ['#ST #NR #ID' '#CL'] p: table b: 2 }
    { s: ['#OS' '#ST #NR #ID'] p: table b: 2 }
    { s: ['#OS' '#OS'] p: table b: 2 }
    { s: ['#ST #NR #ID' '#DOT'] p: table b: 2 }
    { s: '#ZZ' }
  ]

  rule: table: {
    open: [
      { s: ['#ST #NR #ID' '#CL'] p: map b: 2 }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 }
      { s: ['#OS' '#OS'] r: table n: { table_array: 1 } }
      {
        s: ['#ST #NR #ID' '#DOT']
        c: '@table-top-dive-cond'
        p: dive
        b: 2
        u: { top_dive: true }
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        c: '@lte-table-dive'
        n: { table_dive: 1 }
        a: '@table-dive-start'
        g: 'dive,start'
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        n: { table_dive: 1 }
        a: '@table-dive-mid'
        g: 'dive'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        c: '@lte-table-dive'
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-head'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-tail'
        g: 'dive,end'
      }
      {
        s: '#CS'
        p: map
        c: '@lte-table-array-1'
        a: '@table-cs-push'
      }
    ]
    close: [
      { s: ['#OS' '#OS'] r: table b: 2 }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 }
      { s: '#ZZ' }
    ]
  }

  rule: map: {
    open: [
      { s: '#OS' b: 1 }
      {
        s: ['#ST #NR #ID' '#CL']
        c: '@map-is-table-parent'
        p: pair
        b: 2
      }
      { s: ['#OB' '#ST #NR #ID'] b: 1 p: pair }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
      { s: '#ZZ' }
    ]
    close: [
      { s: '#OS' b: 1 }
      { s: '#ZZ' }
    ]
  }

  rule: pair: {
    open: [
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        u: { pair: true }
        a: '@pair-key-set'
      }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
    ]
    close: [
      { s: ['#ST #NR #ID'] b: 1 r: pair }
      { s: ['#CA' '#ST #NR #ID'] b: 1 r: pair }
      { s: ['#OS'] b: 1 }
      { s: ['#CA' '#CB'] c: '@lte-pk' b: 1 }
    ]
  }

  rule: val: close: [
    { s: ['#ST #NR #ID'] b: 1 }
    { s: ['#OS'] b: 1 }
  ]

  rule: elem: close: [
    { s: ['#CA' '#CS'] b: 1 g: comma }
  ]

  rule: dive: {
    open: [
      {
        s: ['#ST #NR #ID' '#DOT']
        p: dive
        n: { dive_key: 1 }
        a: '@dive-key-dot'
      }
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        n: { dive_key: 1 }
        u: { dive_end: true }
      }
    ]
    close: [
      {
        s: ['#ST #NR #ID' '#DOT']
        b: 2
        r: dive
        c: '@lte-dive-key-1'
        n: { dive_key: 0 }
      }
      {}
    ]
  }
}
`
// --- END EMBEDDED toml-grammar.jsonic ---

// TomlOptions holds parser options. Reserved for future use.
type TomlOptions struct{}

// Parse parses a TOML source string and returns the result.
func Parse(src string, opts ...TomlOptions) (any, error) {
	j := MakeJsonic(opts...)
	return j.Parse(src)
}

// MakeJsonic creates a Jsonic instance configured for TOML parsing.
func MakeJsonic(opts ...TomlOptions) *jsonic.Jsonic {
	j := jsonic.Make()
	if err := apply(j); err != nil {
		panic("toml plugin: " + err.Error())
	}
	return j
}

// apply installs the TOML grammar onto the given Jsonic instance.
func apply(j *jsonic.Jsonic) error {
	parsed, err := jsonic.Make().Parse(grammarText)
	if err != nil {
		return fmt.Errorf("parse grammar: %w", err)
	}
	gsMap, ok := parsed.(map[string]any)
	if !ok {
		return fmt.Errorf("grammar is not a map")
	}

	// Strip features not yet implemented by this Go port. Keeping the
	// original grammar text intact (for parity with the TS embed) and
	// pruning at load time lets the TS and Go ports share one source of
	// truth without Go needing the custom string matcher, date regex
	// value-matchers, or NaN/Infinity keyword defs.
	stripUnsupported(gsMap)

	// Register fixed tokens declared under options.fixed.token. Jsonic Go's
	// MapToOptions doesn't apply fixed.token from a grammar-parsed map, so
	// we do it explicitly before Grammar() so rule alts can resolve them.
	registerFixedTokens(j, gsMap)

	gs := &jsonic.GrammarSpec{Ref: makeRefs()}
	if om, ok := gsMap["options"].(map[string]any); ok {
		gs.OptionsMap = om
	}
	if rm, ok := gsMap["rule"].(map[string]any); ok {
		gs.Rule = mapToRules(rm)
	}

	if err := j.Grammar(gs); err != nil {
		return err
	}

	// Patch the rule set to work around a Go-jsonic lexer limitation: its
	// matchMatch only checks alt position 0 when deciding whether a
	// custom-regex token (e.g. #ID) is expected, while the TypeScript
	// version checks the position currently being lexed (tI=0 or tI=1).
	// Inject an always-false dummy alt with #ID at slot 0 into the close
	// states whose real alts need #ID at slot 1, so `b` in `[b]` gets
	// lexed as #ID rather than rejected.
	injectIDLexGuards(j)

	// Patch value defs for NaN/Infinity literals. These can't round-trip
	// through Jsonic's own parse (they'd come back as the strings "NaN" /
	// "Infinity" or as nil), so they're set here in Go where the literals
	// are available directly.
	registerSpecialFloats(j)

	// Install the TOML-specific string matcher. Handles single- and
	// double-quoted strings including the triple-quoted multi-line
	// forms, which the default Jsonic string matcher doesn't.
	registerTomlStringMatcher(j)

	// Install context-aware date/time matchers so that date-shaped bare
	// keys (`2001-02-03 = 1`, `[2002-01-02]`, `a.2001-02-08 = 7`) fall
	// through to #ID lexing instead of being swallowed by the grammar's
	// regex-based isodate / localtime value matchers.
	registerDateMatchers(j)

	return nil
}

func registerFixedTokens(j *jsonic.Jsonic, gsMap map[string]any) {
	om, ok := gsMap["options"].(map[string]any)
	if !ok {
		return
	}
	fixed, ok := om["fixed"].(map[string]any)
	if !ok {
		return
	}
	tokens, ok := fixed["token"].(map[string]any)
	if !ok {
		return
	}
	for name, v := range tokens {
		src, ok := v.(string)
		if !ok {
			continue
		}
		j.Token(name, src)
	}
}

// injectIDLexGuards prepends a never-matching alt that has #ID at
// position 0 to the close alts of rules where the real alts only
// expect #ID at position 1. See comment at call site.
func injectIDLexGuards(j *jsonic.Jsonic) {
	idTin := j.Token("#ID")
	stTin := j.Token("#ST")
	nrTin := j.Token("#NR")
	idSlot := []jsonic.Tin{stTin, nrTin, idTin}

	never := jsonic.AltCond(func(_ *jsonic.Rule, _ *jsonic.Context) bool {
		return false
	})

	for _, name := range []string{"table", "pair"} {
		j.Rule(name, func(rs *jsonic.RuleSpec, _ *jsonic.Parser) {
			dummy := &jsonic.AltSpec{
				S: [][]jsonic.Tin{idSlot},
				C: never,
			}
			rs.PrependClose(dummy)
		})
	}
}

// registerTomlStringMatcher installs the TOML string matcher at a
// priority that lets it pre-empt Jsonic's default string lexer.
func registerTomlStringMatcher(j *jsonic.Jsonic) {
	j.SetOptions(jsonic.Options{
		Lex: &jsonic.LexOptions{
			Match: map[string]*jsonic.MatchSpec{
				"tomlstring": {
					Order: 900000, // above match.value/token (1e6) not reached; below fixed tokens.
					Make:  tomlStringMatcher,
				},
			},
		},
	})
}

// registerSpecialFloats adds TOML's +/- nan and +/- inf keyword values
// alongside the standard true/false/null, since setting Value.Def
// replaces Jsonic's defaults entirely.
func registerSpecialFloats(j *jsonic.Jsonic) {
	posInf := math.Inf(1)
	negInf := math.Inf(-1)
	nan := math.NaN()

	j.SetOptions(jsonic.Options{
		Value: &jsonic.ValueOptions{
			Def: map[string]*jsonic.ValueDef{
				"true":  {Val: true},
				"false": {Val: false},
				"null":  {Val: nil},
				"nan":   {Val: nan},
				"+nan":  {Val: nan},
				"-nan":  {Val: nan},
				"inf":   {Val: posInf},
				"+inf":  {Val: posInf},
				"-inf":  {Val: negInf},
			},
		},
	})
}

func stripUnsupported(gsMap map[string]any) {
	om, ok := gsMap["options"].(map[string]any)
	if !ok {
		return
	}
	// The TypeScript port installs a custom string matcher that handles
	// TOML's triple-quoted literals. The Go port installs its own
	// equivalent in registerTomlStringMatcher(); remove the @-ref so
	// Grammar() doesn't try to resolve it against our ref map.
	if lex, ok := om["lex"].(map[string]any); ok {
		if match, ok := lex["match"].(map[string]any); ok {
			delete(match, "string")
			if len(match) == 0 {
				delete(lex, "match")
			}
		}
	}

	// Go jsonic's MapToOptions treats a non-nil comment.def map as a full
	// replacement of the default line-comment set, so `{slash: null, multi:
	// null}` (intended to remove just those two) silently drops the `#` line
	// comment. Re-add an explicit hash entry to preserve TOML comments.
	if com, ok := om["comment"].(map[string]any); ok {
		if def, ok := com["def"].(map[string]any); ok {
			def["hash"] = map[string]any{"line": true, "start": "#"}
		}
	}
}
