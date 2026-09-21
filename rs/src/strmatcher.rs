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

#[allow(clippy::too_many_lines)]
fn toml_string_matcher(
    lexer: &mut Lexer<'_>,
    _rule: &mut Rule,
    _context: &mut Context,
) -> Option<Token> {
    let point = lexer.point();
    let source: Vec<char> = lexer.remaining().chars().collect();
    let length = source.len();
    if 0 == length {
        return None;
    }

    let delimiter = source[0];
    if '\'' != delimiter && '"' != delimiter {
        return None;
    }

    // Absolute SCALAR index of the local index `index`, for error spans:
    // `bad_span` indexes scalars, and `site.si` is a byte offset.
    let origin = point.site.pos;
    let span = |start: usize, end: usize| (origin + start, origin + end);

    let begin = 0_usize;
    let mut index = 0_usize;
    let mut is_multiline = false;

    if Some(&delimiter) == source.get(1) {
        if Some(&delimiter) != source.get(2) {
            // `""` or `''`: the empty string.
            let text: String = source[..2].iter().collect();
            lexer.advance_chars(2);
            return Some(lexer.token(
                "#ST",
                TIN_ST,
                Value::String(String::new()),
                text,
                end(lexer),
            ));
        }
        index += 2;
        is_multiline = true;
    }

    // A newline immediately after the opening delimiter is trimmed.
    if is_multiline && Some(&'\n') == source.get(index + 1) {
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
        let Some(character) = source.get(index).copied() else {
            let (start, end) = span(begin, index);
            return Some(lexer.bad_span("unterminated_string", start, end));
        };

        if '\n' == character {
            if !is_multiline {
                let (start, end) = span(index, index + 1);
                return Some(lexer.bad_span("unprintable", start, end));
            }
            value.push('\n');
            continue;
        }

        if delimiter == character {
            if is_multiline {
                if Some(&delimiter) != source.get(index + 1) {
                    value.push(delimiter);
                    continue;
                }
                if Some(&delimiter) != source.get(index + 2) {
                    value.push(delimiter);
                    value.push(delimiter);
                    index += 1;
                    continue;
                }
                index += 2;
                // Up to two further delimiters belong to the value, not
                // to the terminator: `""""hello""""` is `"hello"`.
                if Some(&delimiter) == source.get(index + 1) {
                    value.push(delimiter);
                    index += 1;
                }
                if Some(&delimiter) == source.get(index + 1) {
                    value.push(delimiter);
                    index += 1;
                }
            }
            index += 1;
            closed = true;
            break;
        }

        if is_control_other_than_tab(character) {
            let (start, end) = span(index, index + 1);
            return Some(lexer.bad_span("unprintable", start, end));
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
        let escape = source.get(index).copied();

        if let Some(replacement) = escape.and_then(escape_char) {
            value.push(replacement);
            continue;
        }

        match escape {
            Some('x') => {
                index += 1;
                let rest: String = source[index.min(length)..].iter().collect();
                let Some(code) = two_hex(&rest) else {
                    let (start, end) = span(index.saturating_sub(2), index + 2);
                    return Some(lexer.bad_span("invalid_ascii", start, end));
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
                    match source.get(index).copied() {
                        Some(digit) if is_hexadecimal_lax(digit) => digits.push(digit),
                        _ => {
                            let (start, end) = span(begin_unicode, index);
                            return Some(lexer.bad_span("invalid_unicode", start, end));
                        }
                    }
                }
                // Range-checked BEFORE the code point is built, because
                // the canonical `String.fromCodePoint` THROWS above
                // 0x10FFFF, so `\UFFFFFFFF` left the matcher as an
                // uncaught internal error wearing a diagnostic's clothes.
                let code = hex_prefix(&digits);
                let Some(code) = code.filter(|code| *code <= 0x0010_FFFF) else {
                    let (start, end) = span(begin_unicode, index);
                    return Some(lexer.bad_span("invalid_unicode", start, end));
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
                    match source.get(index + 1).copied() {
                        Some(' ' | '\t' | '\n') => index += 1,
                        Some('\r') if Some(&'\n') == source.get(index + 2) => index += 2,
                        _ => break,
                    }
                }
            }
            _ => value.push(UNKNOWN_ESCAPE),
        }
    }

    if !closed {
        let (start, end) = span(begin, index);
        return Some(lexer.bad_span("unterminated_string", start, end));
    }

    let text: String = source[begin..index.min(length)].iter().collect();
    lexer.advance_chars(index.min(length));
    Some(lexer.token("#ST", TIN_ST, Value::String(value), text, end(lexer)))
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
