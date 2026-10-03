// Copyright (c) 2021-2026 Richard Rodger, MIT License

package tabnastoml

import (
	"encoding/json"
	"fmt"

	jsonic "github.com/tabnas/jsonic/go"
)

// Object nodes in this port are insertion-ordered *jsonic.OrderedMap values,
// matching the engine's default object node (and the TS port's plain-object
// insertion order). newMap allocates a fresh one; asMap type-asserts a node
// to *OrderedMap. Container nodes flowing in from the shared engine (the map
// and pair rules) are already OrderedMaps, so building ours the same way lets
// @table-bc / the dive+array handlers merge and index them uniformly.
func newMap() *jsonic.OrderedMap { return jsonic.NewOrderedMap() }

func asMap(v any) (*jsonic.OrderedMap, bool) {
	om, ok := v.(*jsonic.OrderedMap)
	return om, ok
}

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
			r.Node = newMap()
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
			child, okc := asMap(r.Child.Node)
			node, okn := asMap(r.Node)
			if !okc || !okn {
				return
			}
			for _, k := range child.Keys {
				node.Set(k, child.Vals[k])
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
			if node, ok := asMap(r.Node); ok {
				node.Set(key, r.Child.Node)
			}
		}),

		// --- Alt actions ---
		//
		// The five header actions move along a `[a.b.c]` or `[[a.b.c]]`
		// header one segment at a time, as the canonical port does: a
		// `#DOT`-terminated segment DESCENDS through the key, a
		// `#CS`-terminated one DEFINES it, and tableAt / arrayAt below
		// answer with the node or refuse with `toml_key_conflict`.

		"@table-dive-start": jsonic.AltAction(func(r *jsonic.Rule, ctx *jsonic.Context) {
			key := tokenString(r.O0)
			parent, ok := asMap(r.Parent.Node)
			if !ok {
				return
			}
			if r.N["table_array"] > 0 {
				if arr, ok := valueAt(parent, key).([]any); ok {
					// `[[a.b]]` walks into the LAST table of `a`, growing
					// it by an empty table when it has none.
					last, _ := lastTable(arr, parent, key)
					r.Node = last
					return
				}
			}
			// A plain-table dive into an existing array of tables walks
			// THROUGH it, which is how `[[x]]` followed by `[x.y]` works.
			land(r, tableAt(parent, key, ctx, DESCEND), parent, key)
		}),

		"@table-dive-mid": jsonic.AltAction(func(r *jsonic.Rule, ctx *jsonic.Context) {
			key := tokenString(r.O0)
			if _, ok := r.Prev.Node.([]any); ok {
				// Descending through an array of tables lands in its last
				// table, which an intervening `[[a.b]]` may have left
				// holding an array under this key; that array is kept, so
				// the next @table-cs-push can append to it.
				owner, arrKey, arr := arrayHome(r.Prev)
				last, arr := lastTable(arr, owner, arrKey)
				r.Prev.Node = arr
				land(r, tableAt(last, key, ctx, DESCEND), last, key)
				return
			}
			prev, ok := asMap(r.Prev.Node)
			if !ok {
				return
			}
			land(r, tableAt(prev, key, ctx, DESCEND), prev, key)
		}),

		"@table-key-cs-head": jsonic.AltAction(func(r *jsonic.Rule, ctx *jsonic.Context) {
			key := tokenString(r.O0)
			parent, ok := asMap(r.Parent.Node)
			if !ok {
				return
			}
			if r.N["table_array"] > 0 {
				land(r, arrayAt(parent, key, ctx), parent, key)
				return
			}
			land(r, tableAt(parent, key, ctx, DEFINE), parent, key)
		}),

		"@table-key-cs-tail": jsonic.AltAction(func(r *jsonic.Rule, ctx *jsonic.Context) {
			key := tokenString(r.O0)
			var prev *jsonic.OrderedMap
			if _, ok := r.Prev.Node.([]any); ok {
				// The segment before this one descended through an array
				// of tables; the key is defined in its LAST table, as a
				// table for `[a.b.c]` and as an array of tables for
				// `[[a.b.c]]`.
				owner, arrKey, arr := arrayHome(r.Prev)
				last, arr := lastTable(arr, owner, arrKey)
				r.Prev.Node = arr
				prev = last
			} else if m, ok := asMap(r.Prev.Node); ok {
				prev = m
			} else {
				return
			}
			if r.N["table_array"] > 0 {
				land(r, arrayAt(prev, key, ctx), prev, key)
				return
			}
			land(r, tableAt(prev, key, ctx, DEFINE), prev, key)
		}),

		"@table-cs-push": jsonic.AltAction(func(r *jsonic.Rule, ctx *jsonic.Context) {
			arr, ok := r.Prev.Node.([]any)
			if !ok {
				// `[[a]]` where `a` is already a scalar. The array itself
				// is produced by arrayAt above, so reaching a non-array
				// here means the key conflicts.
				keyConflict(ctx, tokenString(r.O0), describe(r.Prev.Node))
			}
			newM := newMap()
			arr = append(arr, newM)
			r.Prev.Node = arr
			// The array also lives in its owning map; writing back there
			// keeps both views consistent after slice growth.
			if owner, ok := asMap(r.Prev.U["arr_parent"]); ok {
				if arrKey, ok := r.Prev.U["arr_key"].(string); ok {
					owner.Set(arrKey, arr)
				}
			}
			r.Node = newM
		}),

		"@pair-key-set": jsonic.AltAction(func(r *jsonic.Rule, _ *jsonic.Context) {
			if r.O0 != nil && r.O0 != jsonic.NoToken {
				r.EnsureU()["key"] = r.O0.Val
			}
		}),

		// A dotted key inside a table body or an inline table:
		// `a.b = 1` descends through `a`, so an existing table or array
		// passes and a value under that name is a conflict, exactly as
		// `[a.b]` would find it.
		//
		// A dotted key is a replace loop (tabnas/toml#78): each `key .`
		// segment re-enters `dive` in the same frame, so rule depth stays
		// what one segment needs however long the key. The first segment
		// descends from the table the key is in, the parent's node; each
		// later one from the table the segment before it reached, handed
		// on as r.Prev.Node when the dive replaced itself, exactly as
		// @table-dive-mid reads a header's. It used to push a dive per
		// segment, so rule depth grew with the key.
		"@dive-key-dot": jsonic.AltAction(func(r *jsonic.Rule, ctx *jsonic.Context) {
			key := tokenString(r.O0)
			from := r.Parent.Node
			if continuesKey(r) {
				from = r.Prev.Node
			}
			parent, ok := asMap(from)
			if !ok {
				return
			}
			r.Node = tableAt(parent, key, ctx, DESCEND)
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

// continuesKey reports whether this dive continues the dotted key the dive
// before it began: it was reached by `r: dive` from a `key .` segment. A
// dive pushed by a pair or a map begins a key, and so does one reached
// through the close loop from a dive that ENDED a key (dive_end), which
// takes the next dotted key without returning to the pair.
func continuesKey(r *jsonic.Rule) bool {
	prev := r.Prev
	return prev != nil && prev != jsonic.NoRule && prev.Name == "dive" && prev.U["dive_end"] == nil
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

// Whether an existing array of tables is a legitimate thing to land on.
//
// A header like `[fruit.variety]` walks THROUGH `fruit` and DEFINES
// `variety`, and the two positions have opposite rules. Descending through
// an array of tables is how `[[x]]` followed by `[x.y]` works, and four
// valid corpus documents rely on it. Landing on one as the thing being
// defined is `[x]` trying to redefine `[[x]]`: invalid TOML.
//
// The grammar already separates the two: `#DOT`-terminated segments are
// intermediate, `#CS`-terminated ones are final.
const (
	DESCEND = true
	DEFINE  = false
)

// keyConflict is the DIAGNOSED refusal to redefine a key, raised as the
// canonical port raises it: a `toml_key_conflict` error at the current
// token. The engine passes a panicked *TabnasError through with its code
// intact (tabnas/parser go/parser.go, startParse), rebuilt through its
// normal funnel so the error carries the source excerpt, the rule stack
// and the hint registered in registerErrorMessages. Anything else that
// panics inside an action still becomes `internal`, so this is the one
// shape a grammar action may raise.
//
// Detail is rendered here rather than left to the template, because the
// funnel re-renders the template without this action's details.
func keyConflict(ctx *jsonic.Context, key, why string) {
	tkn := ctx.T0
	if tkn == nil {
		tkn = jsonic.NoToken
	}
	panic(&jsonic.JsonicError{
		Code:   "toml_key_conflict",
		Detail: fmt.Sprintf("cannot define %s, %s", key, why),
		Src:    tkn.Src,
		Pos:    tkn.SI,
		Row:    tkn.RI,
		Col:    tkn.CI,
	})
}

// describe says what a key already holds, in the canonical port's words:
// `it already has the value ${JSON.stringify(existing)}`.
func describe(existing any) string {
	raw, err := json.Marshal(existing)
	if err != nil {
		return fmt.Sprintf("it already has the value %v", existing)
	}
	return "it already has the value " + string(raw)
}

// valueAt is the value under key, or nil when the table has none.
func valueAt(container *jsonic.OrderedMap, key string) any {
	existing, _ := container.Get(key)
	return existing
}

// tableAt is the table under key in container, or a DIAGNOSED refusal to
// descend into something that is not one.
//
// TOML forbids redefining a key, so `a = {b = 1, b.c = 2}` and `a = 1`
// followed by `[a.b]` are invalid documents. This port used to walk into
// the scalar and overwrite it with a fresh table, which is silent data
// loss on an invalid document; AGENTS.md counts only a `.code`-bearing
// error as a conformant rejection, and this is where the Go row of
// test/conformance.tsv catches up with the TypeScript one.
//
// An existing array passes when descending (`[[x]]` then `[x.y]`) and
// conflicts when defining (`[[x]]` then `[x]`); an existing table passes
// either way; anything else is a value, and a value is not a table.
func tableAt(container *jsonic.OrderedMap, key string, ctx *jsonic.Context, descend bool) any {
	existing := valueAt(container, key)
	if existing == nil {
		m := newMap()
		container.Set(key, m)
		return m
	}
	if arr, ok := existing.([]any); ok {
		if descend {
			return arr
		}
		// `[[fruit.variety]]` then `[fruit.variety]`. Both ports used to
		// accept this and both DESTROYED data doing it, in opposite
		// directions: TypeScript kept the array and silently dropped the
		// second table's contents, this one replaced the array with the
		// second table and dropped the first.
		keyConflict(ctx, key, "it is already an array of tables")
	}
	if m, ok := asMap(existing); ok {
		return m
	}
	keyConflict(ctx, key, describe(existing))
	return nil
}

// arrayAt is the array of tables under key in container, or a DIAGNOSED
// refusal to append to something that is not an array: `[[a]]` after
// `a = 1`, or after `[a]`.
func arrayAt(container *jsonic.OrderedMap, key string, ctx *jsonic.Context) []any {
	existing := valueAt(container, key)
	if arr, ok := existing.([]any); ok {
		return arr
	}
	if existing == nil {
		arr := []any{}
		container.Set(key, arr)
		return arr
	}
	keyConflict(ctx, key, describe(existing))
	return nil
}

// land records where a header segment now stands. An array of tables is
// remembered with the map that owns it and its key, because a Go slice
// header does not share through a map value: an append made later has to
// be written back where the array lives for the next segment to see it.
func land(r *jsonic.Rule, node any, owner *jsonic.OrderedMap, key string) {
	r.Node = node
	if _, ok := node.([]any); ok {
		u := r.EnsureU()
		u["arr_parent"] = owner
		u["arr_key"] = key
	}
}

// arrayHome re-reads the array the previous segment landed on from the
// map that owns it (see land), so that this segment appends to the live
// array rather than to a stale copy of its slice header.
func arrayHome(prev *jsonic.Rule) (owner *jsonic.OrderedMap, key string, arr []any) {
	owner, _ = asMap(prev.U["arr_parent"])
	key, _ = prev.U["arr_key"].(string)
	if owner != nil {
		arr, _ = valueAt(owner, key).([]any)
	} else {
		arr, _ = prev.Node.([]any)
	}
	return owner, key, arr
}

// lastTable is the last table of an array of tables, growing the array by
// an empty table when it has none and writing the grown array back to its
// owner. The canonical `last ? last : (arr.push(node()), arr[arr.length - 1])`.
func lastTable(arr []any, owner *jsonic.OrderedMap, key string) (*jsonic.OrderedMap, []any) {
	if n := len(arr); n > 0 {
		if last, ok := asMap(arr[n-1]); ok {
			return last, arr
		}
	}
	last := newMap()
	arr = append(arr, last)
	if owner != nil {
		owner.Set(key, arr)
	}
	return last, arr
}
