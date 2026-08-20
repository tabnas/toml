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

		// MALFORMED UTF-8. `utf8.RuneCountInString` counts each invalid
		// byte as ONE rune (RuneError, width 1), and this engine passes
		// invalid input through rather than rejecting it, so that reaches
		// the scan. A "skip continuation bytes" test would count a stray
		// 0x80 as zero and report the later error a column early — which
		// the first version of this repair did. Raised in review.
		//
		// No `ts` column: a JavaScript string cannot hold a lone 0x80 at
		// all, so there is no TypeScript answer to agree or disagree
		// with. The requirement here is only that Go matches its own
		// stated unit.
		//
		// Only the first of the two discriminates. 0x80 matches the
		// continuation pattern `10xxxxxx`, so the rejected first repair
		// answered 6 for it; 0xFF does not match that pattern and was
		// counted either way. The second row is kept as the control that
		// says so — "an invalid byte" and "a continuation byte" are not
		// the same set, and a fix aimed at one must not be read as
		// covering the other.
		{"stray continuation", "[\"a\x80b\" 1]", 7, 0},
		{"invalid 0xff", "[\"a\xffb\" 1]", 7, 0},
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
		if 0 == c.ts {
			// A row with no TypeScript counterpart: assert Go alone.
			if o.Col != c.col {
				t.Errorf("%s: %q col = %d, want %d — an invalid byte is ONE "+
					"rune to utf8.RuneCountInString, and the column has to "+
					"agree with it.", c.label, c.src, o.Col, c.col)
			}
			continue
		}
		if o.Col != c.col {
			t.Errorf("%s: %q col = %d, want %d (TypeScript says %d). "+
				"A column ahead of the want by the character's extra BYTES "+
				"means the scan is counting bytes again.",
				c.label, c.src, o.Col, c.col, c.ts)
		}
	}
}
