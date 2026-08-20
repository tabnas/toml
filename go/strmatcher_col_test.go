// Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// Error COLUMNS after a non-ASCII character in a string.
//
// This port brings its own string matcher, and a custom matcher owns the
// arithmetic the engine's own matchers do for it: SI advances in BYTES
// and CI in RUNES. tomlStringMatcher's scan loop walked the source a byte
// at a time — right for reassembling the value — and incremented CI once
// per byte. So a 2-byte `é` charged two columns and an astral character
// four, and every diagnostic after a non-ASCII character in a string
// pointed past where the problem was.
//
// Found by the fleet parity probe (admin tasks/ax-parity-probe.js), which
// reported this port's `col` running AHEAD of TypeScript's by exactly the
// extra bytes of the preceding character. Neither the engine nor jsonic
// does this — both were measured on the same inputs first, and both count
// runes — so it was this file.
//
// ts/test/toml.test.ts 'error columns count characters, not bytes' asserts
// the same five inputs. The astral row is the only one where the two
// answers differ, and that difference is the recorded engine divergence:
// TypeScript counts UTF-16 units (an astral character is 2) and Go counts
// runes (1). See parser/DIVERGENCE.md, "Column positions for astral
// characters".

import (
	"encoding/json"
	"testing"
)

func TestStringErrorColumnsCountRunesNotBytes(t *testing.T) {
	for _, c := range []struct {
		label string
		src   string
		col   int
		ts    int // what the TypeScript half asserts, for the reader
	}{
		// Control: pure ASCII, where bytes and runes coincide. Without
		// it, "columns are runes" is also satisfied by never counting.
		{"ascii", "[a b]", 2, 2},

		// 2 and 3 bytes, 1 rune, 1 UTF-16 unit: both ports agree.
		{"latin1", "[\"é\" 1]", 5, 5},
		{"bmp", "[\"€\" 1]", 5, 5},

		// 4 bytes, 1 rune, TWO UTF-16 units: the recorded divergence, and
		// the only row where the two halves differ.
		{"astral", "[\"\U0001F600\" 1]", 5, 6},
		{"mixed", "[\"ab\U0001F600cd\" 1]", 9, 10},
	} {
		_, err, panicked := safeParse(c.src)
		if err == nil {
			t.Errorf("%s: %q parsed, expected a diagnostic", c.label, c.src)
			continue
		}
		if panicked {
			t.Errorf("%s: %q panicked: %v", c.label, c.src, err)
			continue
		}
		b, mErr := json.Marshal(err)
		if mErr != nil {
			t.Fatalf("%s: marshal: %v", c.label, mErr)
		}
		var o struct {
			Col   int            `json:"col"`
			Token map[string]any `json:"token"`
		}
		if uErr := json.Unmarshal(b, &o); uErr != nil {
			t.Fatalf("%s: unmarshal: %v", c.label, uErr)
		}
		if o.Col != c.col {
			t.Errorf("%s: %q col = %d, want %d (TypeScript says %d). "+
				"A column ahead of the want by the character's extra BYTES "+
				"means the scan is counting bytes again.",
				c.label, c.src, o.Col, c.col, c.ts)
		}
	}
}
