// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License
//
// Adapted from https://github.com/huan231/toml-nodejs/blob/master/src/tokenizer.ts
// Copyright (c) 2022 Jan Szybowski, MIT License

//! TOML's basic, literal, multi-line basic and multi-line literal
//! strings, with the escape rules from <https://toml.io/en/v1.0.0#string>.
//!
//! Installed through the grammar's `options.lex.match.string.make`
//! reference, so it pre-empts the engine's own string lexer for `"` and
//! `'` and leaves every other character alone.
//!
//! COLUMNS COME FROM THE ENGINE. The scan works in Unicode scalar values
//! and hands the engine a COUNT, which `advance_chars` turns into rows and
//! columns with the same arithmetic every builtin matcher uses. The Go
//! port walks bytes and owns that arithmetic itself, which is why it needs
//! `strmatcher_col_test.go`; here there is no second implementation of it
//! to drift.

use std::cell::Cell;

use tabnas::{Context, ImperativeLexMatcher, Lexer, Options, Rule, Token, Value, TIN_ST};

/// The unknown-escape sentinel. The canonical matcher keeps an unknown
/// escape as U+001B rather than failing, so a document that carries one
/// parses the same in every port.
const UNKNOWN_ESCAPE: char = '\u{1b}';

/// The `make` half of the grammar's `@make-toml-string-matcher`: the
/// factory the engine calls once with the resolved options, answering the
/// matcher it then runs per token.
pub(crate) fn make_toml_string_matcher(_options: &Options) -> Option<ImperativeLexMatcher> {
    Some(std::sync::Arc::new(toml_string_matcher))
}

fn escape_char(escape: char) -> Option<char> {
    match escape {
        'b' => Some('\u{8}'),
        't' => Some('\t'),
        'n' => Some('\n'),
        'f' => Some('\u{c}'),
        'r' => Some('\r'),
        '"' => Some('"'),
        '\\' => Some('\\'),
        _ => None,
    }
}

fn is_control_other_than_tab(character: char) -> bool {
    (character < '\u{20}' && character != '\t') || character == '\u{7f}'
}

/// The canonical `isHexadecimal`, which accepts every ASCII LETTER rather
/// than only `a`-`f`. That is deliberate rather than an oversight to
/// repair: the range check below is what rejects `\uZZZZ`, and the digits
/// a lax scan lets through are then read by [`hex_prefix`], which stops at
/// the first non-hex character exactly as the canonical `parseInt` does.
/// Narrowing this to real hex digits would change which documents parse.
fn is_hexadecimal_lax(character: char) -> bool {
    character.is_ascii_alphanumeric()
}

/// The value `parseInt(text, 16)` produces: the longest hexadecimal
/// PREFIX, or `None` for no leading hex digit at all.
///
/// Saturated at `u32::MAX`, not at 2^32: the only question asked of the
/// answer is whether it exceeds the Unicode range, and a clamp AT 2^32
/// would truncate to 0 on the cast and read `\UFFFFFFFF`-and-longer as
/// U+0000. Today nothing reaches it, because `\U` takes exactly eight
/// digits and 0xFFFFFFFF is already the largest, but a clamp that is
/// wrong when it fires is not a guard.
fn hex_prefix(text: &str) -> Option<u32> {
    let mut value: u64 = 0;
    let mut seen = false;
    for character in text.chars() {
        let Some(digit) = character.to_digit(16) else {
            break;
        };
        seen = true;
        value = (value * 16 + u64::from(digit)).min(u64::from(u32::MAX));
    }
    #[allow(clippy::cast_possible_truncation)]
    seen.then_some(value as u32)
}

/// Exactly two hexadecimal digits, the `\xHH` form. Strict, because
/// `parseInt` is not a validator: it stops at the first non-hex character
/// and returns what it read, so `\xAg` decoded as U+000A and swallowed the
/// `g`, and `\xA"` decoded as U+000A and swallowed the CLOSING QUOTE.
fn two_hex(text: &str) -> Option<u32> {
    let digits: Vec<char> = text.chars().take(2).collect();
    (2 == digits.len() && digits.iter().all(char::is_ascii_hexdigit))
        .then(|| u32::from_str_radix(&digits.iter().collect::<String>(), 16).ok())
        .flatten()
}

/// How many characters of source a string scan first copies. The window
/// doubles whenever the scan reads past it, so a string costs time in
/// proportion to its own length rather than to the rest of the document.
const FIRST_WINDOW: usize = 256;

fn toml_string_matcher(
    lexer: &mut Lexer<'_>,
    _rule: &mut Rule,
    _context: &mut Context,
) -> Option<Token> {
    // Look before copying anything. The engine consults this matcher at
    // nearly every token, and copying the rest of the source on each call
    // made a parse quadratic in its length: 4,000 tables took 23 seconds.
    let delimiter = lexer.remaining().chars().next()?;
    if '\'' != delimiter && '"' != delimiter {
        return None;
    }
    // Absolute SCALAR index of a string's first character, for error
    // spans: `bad_span` indexes scalars, and `site.si` is a byte offset.
    let origin = lexer.point().site.pos;
    let mut want = FIRST_WINDOW;
    loop {
        let window = Window::new(lexer.forward(want), want);
        let scanned = scan(&window, delimiter, origin);
        if window.short.get() {
            want = want.saturating_mul(2);
            continue;
        }
        return Some(match scanned {
            Scan::Bad { why, start, end } => lexer.bad_span(why, start, end),
            Scan::String {
                value,
                text,
                advance,
            } => {
                lexer.advance_chars(advance);
                lexer.token("#ST", TIN_ST, Value::String(value), text, end(lexer))
            }
        });
    }
}

/// The source from the cursor on, as far as a scan has needed it: all of
/// the rest of it when `whole`, else a prefix. Reading past the end of a
/// prefix marks the window `short`, and the scan is then repeated over a
/// longer one, so every answer is the one the whole source gives.
struct Window {
    chars: Vec<char>,
    whole: bool,
    short: Cell<bool>,
}

impl Window {
    fn new(text: &str, want: usize) -> Window {
        let chars: Vec<char> = text.chars().collect();
        // `forward` answers fewer than `want` only at the end of source.
        let whole = chars.len() < want;
        Window {
            chars,
            whole,
            short: Cell::new(false),
        }
    }

    /// The character at `index`.
    fn get(&self, index: usize) -> Option<char> {
        let found = self.chars.get(index).copied();
        if found.is_none() && !self.whole {
            self.short.set(true);
        }
        found
    }

    /// How many characters the rest of the source has, as far as this
    /// window can say: a prefix does not know, so it answers "enough".
    fn len(&self) -> usize {
        if self.whole {
            self.chars.len()
        } else {
            usize::MAX
        }
    }

    /// The characters from `begin` to `end`.
    fn text(&self, begin: usize, end: usize) -> String {
        if end > self.chars.len() && !self.whole {
            self.short.set(true);
        }
        let end = end.min(self.chars.len());
        self.chars[begin.min(end)..end].iter().collect()
    }
}

/// What a scan found: a string token, or a bad one.
enum Scan {
    Bad {
        why: &'static str,
        start: usize,
        end: usize,
    },
    String {
        value: String,
        text: String,
        advance: usize,
    },
}

/// Scan the string that opens with `delimiter` at the head of `source`.
/// `origin` is where that is in the whole source, for error spans.
#[allow(clippy::too_many_lines)]
fn scan(source: &Window, delimiter: char, origin: usize) -> Scan {
    let length = source.len();
    let span = |why: &'static str, start: usize, end: usize| Scan::Bad {
        why,
        start: origin + start,
        end: origin + end,
    };

    let begin = 0_usize;
    let mut index = 0_usize;
    let mut is_multiline = false;

    if Some(delimiter) == source.get(1) {
        if Some(delimiter) != source.get(2) {
            // `""` or `''`: the empty string.
            return Scan::String {
                value: String::new(),
                text: source.text(0, 2),
                advance: 2,
            };
        }
        index += 2;
        is_multiline = true;
    }

    // A newline immediately after the opening delimiter is trimmed.
    if is_multiline && Some('\n') == source.get(index + 1) {
        index += 1;
    }

    let mut value = String::new();
    // `closed` records that the loop stopped ON the closing delimiter,
    // which is the only legitimate way out. Without it, an unterminated
    // string becomes a VALID token holding whatever had been read, and
    // `a = "abc ` parses with a closing quote the source never had.
    let mut closed = false;

    while index < length {
        index += 1;
        let Some(character) = source.get(index) else {
            return span("unterminated_string", begin, index);
        };

        if '\n' == character {
            if !is_multiline {
                return span("unprintable", index, index + 1);
            }
            value.push('\n');
            continue;
        }

        if delimiter == character {
            if is_multiline {
                if Some(delimiter) != source.get(index + 1) {
                    value.push(delimiter);
                    continue;
                }
                if Some(delimiter) != source.get(index + 2) {
                    value.push(delimiter);
                    value.push(delimiter);
                    index += 1;
                    continue;
                }
                index += 2;
                // Up to two further delimiters belong to the value, not
                // to the terminator: `""""hello""""` is `"hello"`.
                if Some(delimiter) == source.get(index + 1) {
                    value.push(delimiter);
                    index += 1;
                }
                if Some(delimiter) == source.get(index + 1) {
                    value.push(delimiter);
                    index += 1;
                }
            }
            index += 1;
            closed = true;
            break;
        }

        if is_control_other_than_tab(character) {
            return span("unprintable", index, index + 1);
        }

        if '\'' == delimiter {
            // Literal strings take no escapes.
            value.push(character);
            continue;
        }

        if '\\' != character {
            value.push(character);
            continue;
        }

        index += 1;
        let escape = source.get(index);

        if let Some(replacement) = escape.and_then(escape_char) {
            value.push(replacement);
            continue;
        }

        match escape {
            Some('x') => {
                index += 1;
                // `two_hex` reads two characters, and only two.
                let rest: String = (index..index + 2).map_while(|at| source.get(at)).collect();
                let Some(code) = two_hex(&rest) else {
                    return span("invalid_ascii", index.saturating_sub(2), index + 2);
                };
                value.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
                // The loop's own step takes the second digit.
                index += 1;
            }
            Some(marker @ ('u' | 'U')) => {
                let begin_unicode = index;
                let size = if 'u' == marker { 4 } else { 8 };
                let mut digits = String::new();
                for _ in 0..size {
                    index += 1;
                    match source.get(index) {
                        Some(digit) if is_hexadecimal_lax(digit) => digits.push(digit),
                        _ => return span("invalid_unicode", begin_unicode, index),
                    }
                }
                // Range-checked BEFORE the code point is built, because
                // the canonical `String.fromCodePoint` THROWS above
                // 0x10FFFF, so `\UFFFFFFFF` left the matcher as an
                // uncaught internal error wearing a diagnostic's clothes.
                let code = hex_prefix(&digits);
                let Some(code) = code.filter(|code| *code <= 0x0010_FFFF) else {
                    return span("invalid_unicode", begin_unicode, index);
                };
                // A lone surrogate has no Rust representation and folds to
                // U+FFFD, the engine-wide behaviour recorded in
                // parser/DIVERGENCE.md.
                value.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
            Some(' ' | '\t' | '\n' | '\r') if is_multiline => {
                // A line-ending backslash trims every whitespace character
                // up to the next non-whitespace one.
                loop {
                    match source.get(index + 1) {
                        Some(' ' | '\t' | '\n') => index += 1,
                        Some('\r') if Some('\n') == source.get(index + 2) => index += 2,
                        _ => break,
                    }
                }
            }
            _ => value.push(UNKNOWN_ESCAPE),
        }
    }

    if !closed {
        return span("unterminated_string", begin, index);
    }

    let advance = index.min(length);
    Scan::String {
        value,
        text: source.text(begin, advance),
        advance,
    }
}

/// A string token's point is the cursor AFTER the string, not before it.
///
/// That reads like a bug and is the canonical behaviour: the TypeScript
/// matcher writes its finished `sI` / `rI` / `cI` back into `pnt` and only
/// then builds the token, and the Go matcher does the same, so in both
/// ports a `#ST` from this matcher carries the position where the string
/// ENDS. Diagnostics about the token after a quoted key point there, which
/// is what `strmatcher_col_test.go` and the TypeScript
/// 'error columns count characters, not bytes' test both measure. A token
/// built from the point captured on entry moves every one of those columns
/// back to the opening quote.
fn end(lexer: &Lexer<'_>) -> tabnas::Point {
    lexer.point()
}
