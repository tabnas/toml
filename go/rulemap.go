// Copyright (c) 2021-2026 Richard Rodger, MIT License

package toml

import (
	jsonic "github.com/tabnas/jsonic/go"
)

// mapToRules converts a parsed grammar `rule` map into the typed
// GrammarRuleSpec map expected by jsonic.Grammar(). Mirrors the
// (unexported) mapToGrammarRules in the jsonic Go package.
func mapToRules(ruleMap map[string]any) map[string]*jsonic.GrammarRuleSpec {
	rules := make(map[string]*jsonic.GrammarRuleSpec, len(ruleMap))
	for name, v := range ruleMap {
		rm, ok := v.(map[string]any)
		if !ok {
			continue
		}
		spec := &jsonic.GrammarRuleSpec{}
		if open, ok := rm["open"]; ok {
			spec.Open = parseAlts(open)
		}
		if close, ok := rm["close"]; ok {
			spec.Close = parseAlts(close)
		}
		rules[name] = spec
	}
	return rules
}

func parseAlts(v any) []*jsonic.GrammarAltSpec {
	arr, ok := v.([]any)
	if !ok {
		return nil
	}
	alts := make([]*jsonic.GrammarAltSpec, 0, len(arr))
	for _, item := range arr {
		m, ok := item.(map[string]any)
		if !ok {
			continue
		}
		alts = append(alts, mapToAlt(m))
	}
	return alts
}

func mapToAlt(m map[string]any) *jsonic.GrammarAltSpec {
	alt := &jsonic.GrammarAltSpec{}

	// s: string | []string. Grammar parsing yields []any for arrays; the
	// upstream resolveTokenField only accepts []string, so coerce here.
	if v, ok := m["s"]; ok {
		switch sv := v.(type) {
		case string:
			alt.S = sv
		case []any:
			strs := make([]string, 0, len(sv))
			for _, e := range sv {
				if s, ok := e.(string); ok {
					strs = append(strs, s)
				}
			}
			alt.S = strs
		}
	}
	if v, ok := m["b"]; ok {
		alt.B = v
	}
	if v, ok := m["p"].(string); ok {
		alt.P = v
	}
	if v, ok := m["r"].(string); ok {
		alt.R = v
	}
	if v, ok := m["a"].(string); ok {
		alt.A = v
	}
	if v, ok := m["e"].(string); ok {
		alt.E = v
	}
	if v, ok := m["h"].(string); ok {
		alt.H = v
	}
	if v, ok := m["c"]; ok {
		alt.C = v
	}
	if v, ok := m["n"].(map[string]any); ok {
		alt.N = make(map[string]int, len(v))
		for k, val := range v {
			if f, ok := val.(float64); ok {
				alt.N[k] = int(f)
			}
		}
	}
	if v, ok := m["u"].(map[string]any); ok {
		alt.U = v
	}
	if v, ok := m["k"].(map[string]any); ok {
		alt.K = v
	}
	if v, ok := m["g"].(string); ok {
		alt.G = v
	}
	return alt
}
