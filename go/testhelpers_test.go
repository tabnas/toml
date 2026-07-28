// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	jsonic "github.com/tabnas/jsonic/go"
)

// resultMap unwraps a parse-result object node to a plain map[string]any for
// value access in tests. Parsed objects are now insertion-ordered
// *jsonic.OrderedMap values; this returns their underlying Vals (dropping
// order, which the value-oriented assertions here don't care about) and
// still accepts a plain map[string]any for robustness.
func resultMap(v any) map[string]any {
	switch m := v.(type) {
	case *jsonic.OrderedMap:
		return m.Vals
	case map[string]any:
		return m
	}
	return nil
}
