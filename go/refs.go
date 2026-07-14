// Copyright (c) 2021-2026 Richard Rodger, MIT License

package tabnastoml

import (
	jsonic "github.com/tabnas/jsonic/go"
)

// makeRefs builds the function reference map that the grammar file
// references via @-prefixed strings. State-action names
// (@<rule>-<bo|ao|bc|ac>) are auto-wired by Jsonic's Grammar() via
// wireStateActions.
func makeRefs() map[jsonic.FuncRef]any {
	return map[jsonic.FuncRef]any{

		// --- Value-match callbacks (datetime / time) ---

		"@isodate-val":   func(res []string) any { return isodateVal(res) },
		"@localtime-val": func(res []string) any { return localtimeVal(res) },

		// --- State actions (auto-wired by rule name convention) ---

		"@toml-bo": jsonic.StateAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			r.Node = make(map[string]any)
		}),

		"@table-bo": jsonic.StateAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			r.Node = r.Parent.Node
		}),

		"@table-bc": jsonic.StateAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			if r.U["top_dive"] != nil {
				return
			}
			if r.Child == nil || r.Child == jsonic.NoRule {
				return
			}
			child, okc := r.Child.Node.(map[string]any)
			node, okn := r.Node.(map[string]any)
			if !okc || !okn {
				return
			}
			for k, v := range child {
				node[k] = v
			}
		}),

		"@table-ac": jsonic.StateAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			// Reset the dive/array counters on the rule that the parser
			// transitions to after this table closes. Mirrors the TS
			// handler that receives `next` as its third arg.
			next := r.Next
			if next != nil && next != jsonic.NoRule {
				n := next.EnsureN()
				n["table_dive"] = 0
				n["table_array"] = 0
			}
		}),

		"@dive-bc": jsonic.StateAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			if r.U["dive_end"] == nil {
				return
			}
			if r.O0 == nil || r.O0 == jsonic.NoToken {
				return
			}
			key, ok := r.O0.Val.(string)
			if !ok {
				return
			}
			if node, ok := r.Node.(map[string]any); ok {
				node[key] = r.Child.Node
			}
		}),

		// --- Alt actions ---

		"@table-dive-start": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			r.EnsureU()
			key := tokenString(r.O0)
			parent, ok := r.Parent.Node.(map[string]any)
			if !ok {
				return
			}
			existing := parent[key]

			if r.N["table_array"] > 0 {
				if arr, ok := existing.([]any); ok {
					if len(arr) > 0 {
						if last, ok := arr[len(arr)-1].(map[string]any); ok {
							r.Node = last
							return
						}
					}
					newMap := make(map[string]any)
					arr = append(arr, newMap)
					parent[key] = arr
					r.Node = newMap
					return
				}
			}

			// Plain-table dive into an existing array: track it so later
			// handlers can treat it like [[…]]. Mirrors the TS behavior
			// `r.parent.node[key] || {}` where truthy arrays pass through
			// unchanged.
			if arr, ok := existing.([]any); ok {
				r.Node = arr
				r.U["arr_parent"] = parent
				r.U["arr_key"] = key
				return
			}
			if m, ok := existing.(map[string]any); ok {
				r.Node = m
				return
			}
			newMap := make(map[string]any)
			parent[key] = newMap
			r.Node = newMap
		}),

		"@table-dive-mid": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			r.EnsureU()
			key := tokenString(r.O0)
			if _, ok := r.Prev.Node.([]any); ok {
				// Extract array from its actual home in the parent map so
				// appends here are visible to later reads. Go slice headers
				// don't share through map values.
				arrParent, _ := r.Prev.U["arr_parent"].(map[string]any)
				arrKey, _ := r.Prev.U["arr_key"].(string)
				arr, _ := arrParent[arrKey].([]any)
				var last map[string]any
				if len(arr) > 0 {
					last, _ = arr[len(arr)-1].(map[string]any)
				}
				if last == nil {
					last = make(map[string]any)
					arr = append(arr, last)
					if arrParent != nil {
						arrParent[arrKey] = arr
					}
					r.Prev.Node = arr
				}
				// An intervening [[a.b]] leaves last[key] as a []any; keep
				// it so the next table-cs-push can append. Mirrors the TS
				// dive-mid's `r.node = r.prev.node[key] || {}` where a
				// truthy array falls straight through.
				if nextArr, ok := last[key].([]any); ok {
					r.Node = nextArr
					r.U["arr_parent"] = last
					r.U["arr_key"] = key
					return
				}
				next, ok := last[key].(map[string]any)
				if !ok {
					next = make(map[string]any)
					last[key] = next
				}
				r.Node = next
				return
			}
			prev, ok := r.Prev.Node.(map[string]any)
			if !ok {
				return
			}
			// Same array-preservation rule when the previous node was a map
			// rather than a slice (e.g. second [[a.b]] after dive-start
			// returns the first a[0] map whose b already holds an array).
			if arr, ok := prev[key].([]any); ok {
				r.Node = arr
				r.U["arr_parent"] = prev
				r.U["arr_key"] = key
				return
			}
			next, ok := prev[key].(map[string]any)
			if !ok {
				next = make(map[string]any)
				prev[key] = next
			}
			r.Node = next
		}),

		"@table-key-cs-head": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			r.EnsureU()
			key := tokenString(r.O0)
			parent, ok := r.Parent.Node.(map[string]any)
			if !ok {
				return
			}
			existing := parent[key]
			if existing == nil {
				if r.N["table_array"] > 0 {
					arr := []any{}
					parent[key] = arr
					r.Node = arr
					r.U["arr_parent"] = parent
					r.U["arr_key"] = key
				} else {
					m := make(map[string]any)
					parent[key] = m
					r.Node = m
				}
				return
			}
			if arr, ok := existing.([]any); ok {
				r.Node = arr
				r.U["arr_parent"] = parent
				r.U["arr_key"] = key
				return
			}
			r.Node = existing
		}),

		"@table-key-cs-tail": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			r.EnsureU()
			key := tokenString(r.O0)
			if _, ok := r.Prev.Node.([]any); ok {
				arrParent, _ := r.Prev.U["arr_parent"].(map[string]any)
				arrKey, _ := r.Prev.U["arr_key"].(string)
				arr, _ := arrParent[arrKey].([]any)
				var last map[string]any
				if len(arr) > 0 {
					last, _ = arr[len(arr)-1].(map[string]any)
				}
				if last == nil {
					last = make(map[string]any)
					arr = append(arr, last)
					if arrParent != nil {
						arrParent[arrKey] = arr
					}
					r.Prev.Node = arr
				}
				next, ok := last[key].(map[string]any)
				if !ok {
					next = make(map[string]any)
					last[key] = next
				}
				r.Node = next
				return
			}
			prev, ok := r.Prev.Node.(map[string]any)
			if !ok {
				return
			}
			existing := prev[key]
			if existing == nil {
				if r.N["table_array"] > 0 {
					arr := []any{}
					prev[key] = arr
					r.Node = arr
					r.U["arr_parent"] = prev
					r.U["arr_key"] = key
				} else {
					m := make(map[string]any)
					prev[key] = m
					r.Node = m
				}
				return
			}
			if arr, ok := existing.([]any); ok {
				r.Node = arr
				r.U["arr_parent"] = prev
				r.U["arr_key"] = key
				return
			}
			r.Node = existing
		}),

		"@table-cs-push": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			newMap := make(map[string]any)
			if arr, ok := r.Prev.Node.([]any); ok {
				arr = append(arr, newMap)
				r.Prev.Node = arr
				// The array also lives in its parent map; writing back
				// there keeps both views consistent after slice growth.
				if arrParent, ok := r.Prev.U["arr_parent"].(map[string]any); ok {
					if arrKey, ok := r.Prev.U["arr_key"].(string); ok {
						arrParent[arrKey] = arr
					}
				}
			}
			r.Node = newMap
		}),

		"@pair-key-set": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			if r.O0 != nil && r.O0 != jsonic.NoToken {
				r.EnsureU()["key"] = r.O0.Val
			}
		}),

		"@dive-key-dot": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			key := tokenString(r.O0)
			parent, ok := r.Parent.Node.(map[string]any)
			if !ok {
				return
			}
			existing, ok := parent[key].(map[string]any)
			if !ok {
				existing = make(map[string]any)
				parent[key] = existing
			}
			r.Node = existing
		}),

		// --- Conditions ---

		"@table-top-dive-cond": jsonic.AltCond(func(r *jsonic.Rule, _ *jsonic.Context) bool {
			return r.D == 1 && (r.Prev == nil || r.Prev.Name != "table")
		}),

		"@lte-table-dive": jsonic.AltCond(func(r *jsonic.Rule, _ *jsonic.Context) bool {
			return r.Lte("table_dive", 0)
		}),

		"@lte-table-array-1": jsonic.AltCond(func(r *jsonic.Rule, _ *jsonic.Context) bool {
			return r.Lte("table_array", 1)
		}),

		"@lte-dive-key-1": jsonic.AltCond(func(r *jsonic.Rule, _ *jsonic.Context) bool {
			return r.Lte("dive_key", 1)
		}),

		"@lte-pk": jsonic.AltCond(func(r *jsonic.Rule, _ *jsonic.Context) bool {
			return r.Lte("pk", 0)
		}),

		"@map-is-table-parent": jsonic.AltCond(func(r *jsonic.Rule, _ *jsonic.Context) bool {
			return r.Parent != nil && r.Parent.Name == "table"
		}),

		// --- Dynamic push/replace targets ---

		"@table-end-p": func(r *jsonic.Rule, _ *jsonic.Context) string {
			if r.N["table_array"] > 0 {
				return ""
			}
			return "map"
		},

		"@table-end-r": func(r *jsonic.Rule, _ *jsonic.Context) string {
			if r.N["table_array"] > 0 {
				return "table"
			}
			return ""
		},
	}
}

// tokenString returns a token's value as a string.
func tokenString(t *jsonic.Token) string {
	if t == nil || t == jsonic.NoToken {
		return ""
	}
	if s, ok := t.Val.(string); ok {
		return s
	}
	if t.Src != "" {
		return t.Src
	}
	return ""
}
