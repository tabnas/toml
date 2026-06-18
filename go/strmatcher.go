// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License
//
// Adapted from https://github.com/huan231/toml-nodejs/blob/master/src/tokenizer.ts
// Copyright (c) 2022 Jan Szybowski, MIT License

package tabnastoml

import (
	"strconv"
	"strings"

	jsonic "github.com/tabnas/jsonic/go"
)

// tomlStringMatcher returns a LexMatcher that recognises TOML's basic,
// literal, multi-line basic, and multi-line literal strings, with the
// escape rules from https://toml.io/en/v1.0.0#string.
//
// Registered at a priority higher than jsonic's default string matcher
// so it wins for `"` and `'`-delimited inputs but leaves other lexing
// untouched.
func tomlStringMatcher(_ *jsonic.LexConfig, _ *jsonic.Options) jsonic.LexMatcher {
	return func(lex *jsonic.Lex, _ *jsonic.Rule) *jsonic.Token {
		pnt := lex.Cursor()
		src := lex.Src
		srcLen := len(src)

		sI := pnt.SI
		cI := pnt.CI
		rI := pnt.RI
		if sI >= srcLen {
			return nil
		}

		delim := src[sI]
		if delim != '\'' && delim != '"' {
			return nil
		}

		begin := sI
		isMultiline := false

		// Check for `"""` or `'''` opener.
		if sI+1 < srcLen && src[sI+1] == delim {
			if sI+2 >= srcLen || src[sI+2] != delim {
				// `""` or `''`: empty string.
				pnt.SI = sI + 2
				pnt.CI = cI + 2
				return lex.Token("#ST", jsonic.TinST, "", src[begin:sI+2])
			}
			sI += 2
			cI += 2
			isMultiline = true
		}

		// A newline immediately following the opening delimiter is trimmed.
		if isMultiline && sI+1 < srcLen && src[sI+1] == '\n' {
			sI++
			cI = 0
		}

		var b strings.Builder

		for sI < srcLen-1 {
			sI++
			cI++
			c := src[sI]

			switch c {
			case '\n':
				if !isMultiline {
					return lex.Bad("unprintable")
				}
				b.WriteByte('\n')
				cI = 0
				rI++
				continue

			case delim:
				if isMultiline {
					if sI+1 >= srcLen || src[sI+1] != delim {
						b.WriteByte(delim)
						continue
					}
					if sI+2 >= srcLen || src[sI+2] != delim {
						b.WriteByte(delim)
						b.WriteByte(delim)
						sI++
						cI++
						continue
					}
					// Closing """.
					sI += 2
					cI += 2
					// Optionally consume up to two trailing delimiters.
					if sI+1 < srcLen && src[sI+1] == delim {
						b.WriteByte(delim)
						sI++
					}
					if sI+1 < srcLen && src[sI+1] == delim {
						b.WriteByte(delim)
						sI++
					}
				}
				sI++
				cI++
				pnt.SI = sI
				pnt.CI = cI
				pnt.RI = rI
				return lex.Token("#ST", jsonic.TinST, b.String(), src[begin:sI])

			default:
				if sI >= srcLen {
					return lex.Bad("unterminated_string")
				}
				if isControlCharOtherThanTab(c) {
					return lex.Bad("unprintable")
				}
				if delim == '\'' {
					// Literal strings: no escapes.
					b.WriteByte(c)
					continue
				}
				// Basic strings: process escapes.
				if c != '\\' {
					b.WriteByte(c)
					continue
				}
				sI++
				cI++
				if sI >= srcLen {
					return lex.Bad("unterminated_string")
				}
				esc := src[sI]
				if rep, ok := escMap[esc]; ok {
					b.WriteByte(rep)
					continue
				}
				switch esc {
				case 'x':
					if sI+2 >= srcLen {
						return lex.Bad("invalid_ascii")
					}
					cc, err := strconv.ParseInt(src[sI+1:sI+3], 16, 32)
					if err != nil {
						return lex.Bad("invalid_ascii")
					}
					b.WriteRune(rune(cc))
					sI += 2
					cI += 2
				case 'u', 'U':
					size := 4
					if esc == 'U' {
						size = 8
					}
					if sI+size >= srcLen {
						return lex.Bad("invalid_unicode")
					}
					hex := src[sI+1 : sI+1+size]
					for _, h := range []byte(hex) {
						if !isHexadecimal(h) {
							return lex.Bad("invalid_unicode")
						}
					}
					cp, err := strconv.ParseInt(hex, 16, 64)
					if err != nil {
						return lex.Bad("invalid_unicode")
					}
					b.WriteRune(rune(cp))
					sI += size
					cI += size
				default:
					if isMultiline && (esc == ' ' || esc == '\t' || esc == '\n' || esc == '\r') {
						// Line-ending backslash: trim whitespace up to next
						// non-whitespace.
						for sI+1 < srcLen {
							n := src[sI+1]
							switch n {
							case ' ', '\t':
								sI++
								cI++
							case '\n':
								sI++
								rI++
								cI = 0
							case '\r':
								if sI+2 < srcLen && src[sI+2] == '\n' {
									sI += 2
									rI++
									cI = 0
								} else {
									goto doneTrim
								}
							default:
								goto doneTrim
							}
						}
					doneTrim:
						continue
					}
					// Unknown escape: keep as-is (ESC sentinel in TS port;
					// we just keep the raw char for predictable output).
					b.WriteByte('\u001b')
				}
			}
		}

		return lex.Bad("unterminated_string")
	}
}

var escMap = map[byte]byte{
	'b':  '\b',
	't':  '\t',
	'n':  '\n',
	'f':  '\f',
	'r':  '\r',
	'"':  '"',
	'\\': '\\',
}

func isHexadecimal(c byte) bool {
	return ('0' <= c && c <= '9') ||
		('a' <= c && c <= 'f') ||
		('A' <= c && c <= 'F')
}

func isControlCharOtherThanTab(c byte) bool {
	return (c < 0x20 && c != '\t') || c == 0x7f
}
