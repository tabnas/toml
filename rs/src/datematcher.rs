// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

//! Context-aware date / time matchers, and the leading-BOM matcher.
//!
//! A value matcher fires unconditionally, so a date-shaped BARE KEY
//! (`2001-02-03 = 1`, `[2002-01-02]`, `a.2001-02-08 = 7`) would be claimed
//! as a datetime before the `#ID` token matcher ever ran. These matchers
//! answer `#ID` where a key is accepted and `#VL` where it is not, which
//! is the same swap the canonical port installs over the grammar's regexp
//! matchers after the document goes in.

use std::sync::OnceLock;

use regex::Regex;
use tabnas::{Context, Lexer, Rule, RuleState, Tin, Token, Value, TIN_SP, TIN_VL};

use crate::daterange::{isodate_in_range, localtime_in_range};
use crate::values::{isodate_time, localtime_time};

/// The date / time shapes, the same ones the grammar's regexp value
/// matchers recognise.
///
/// Spelled `[0-9]` rather than `\d`. The `regex` crate reads `\d` as the
/// whole Unicode `Nd` category, while the JavaScript original is compiled
/// without the `u` flag and the Go port is RE2, so in both of those `\d`
/// is ASCII `0-9` alone. A Unicode `\d` here claims `٢٠٢٤-٠١-٠١`, and in
/// a key context this matcher emits what it claimed as an `#ID`, so the
/// port accepted a bare key of Arabic-Indic digits that TOML does not
/// allow and that both other runtimes reject as unexpected characters.
pub(crate) fn isodate_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]",
            r"([Tt ][0-9][0-9]:[0-9][0-9](:[0-9][0-9](\.[0-9]+)?)?",
            r"([Zz]|[-+][0-9][0-9]:[0-9][0-9])?)?",
        ))
        .expect("the isodate pattern is a literal and compiles")
    })
}

/// See [`isodate_re`], including why the digits are written out.
pub(crate) fn localtime_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[0-9][0-9]:[0-9][0-9](:[0-9][0-9](\.[0-9]+)?)?")
            .expect("the localtime pattern is a literal and compiles")
    })
}

/// Whether the rule position accepts a bare key, that is, lists `#ID`
/// among its expected tokens.
///
/// Value-producing rules (`val`, `list`, `elem`) never name `#ID`;
/// key-accepting ones (`toml`, `map`, `dive`, `pair`, `table`) do. A
/// custom matcher is not told which slot it is at, so every position of
/// every alternate in the current phase is scanned, which is the same
/// heuristic both other ports use.
fn is_key_context(id_tin: Tin, rule: &Rule) -> bool {
    let alts = if RuleState::Close == rule.state {
        &rule.spec.close
    } else {
        &rule.spec.open
    };
    alts.iter()
        .any(|alt| alt.s.iter().any(|position| position.contains(&id_tin)))
}

/// The whole match and its capture groups, in the shape the `val`
/// callbacks take: group 0 is the match, an absent group is empty.
fn groups(found: &regex::Captures<'_>) -> Vec<String> {
    (0..found.len())
        .map(|index| {
            found
                .get(index)
                .map_or_else(String::new, |group| group.as_str().to_string())
        })
        .collect()
}

fn date_matcher(
    pattern: &'static Regex,
    in_range: fn(&str) -> bool,
    to_value: fn(&[String]) -> Value,
    lexer: &mut Lexer<'_>,
    rule: &mut Rule,
) -> Option<Token> {
    let point = lexer.point();
    // The captures borrow the remaining source, which borrows the lexer;
    // copy them out before anything asks the lexer for a token identity.
    let captured = pattern
        .captures(lexer.remaining())
        .map(|found| groups(&found))?;
    let matched = captured.first().cloned().unwrap_or_default();
    if matched.is_empty() {
        return None;
    }
    let scalars = matched.chars().count();

    let id_tin = lexer.token_tin("#ID");
    let token = if is_key_context(id_tin, rule) {
        // A bare key that merely LOOKS like a date is not a date, so its
        // components are not required to be in range: `2006-01-32 = 1`
        // defines a key, and a range check here would be one applied to a
        // name.
        lexer.token(
            "#ID",
            id_tin,
            Value::String(matched.clone()),
            matched,
            point,
        )
    } else {
        if !in_range(&matched) {
            return Some(lexer.bad_span(
                "invalid_datetime",
                point.site.pos,
                point.site.pos + scalars,
            ));
        }
        let value = to_value(&captured);
        lexer.token("#VL", TIN_VL, value, matched, point)
    };
    lexer.advance_chars(scalars);
    Some(token)
}

pub(crate) fn isodate_matcher(
    lexer: &mut Lexer<'_>,
    rule: &mut Rule,
    _context: &mut Context,
) -> Option<Token> {
    date_matcher(
        isodate_re(),
        isodate_in_range,
        |groups| isodate_time(groups).to_value(),
        lexer,
        rule,
    )
}

pub(crate) fn localtime_matcher(
    lexer: &mut Lexer<'_>,
    rule: &mut Rule,
    _context: &mut Context,
) -> Option<Token> {
    date_matcher(
        localtime_re(),
        localtime_in_range,
        |groups| localtime_time(groups).to_value(),
        lexer,
        rule,
    )
}

/// The UTF-8 byte order mark, U+FEFF.
const BOM: char = '\u{feff}';

/// Eat a single BOM at the very start of the source.
///
/// A TOML document is allowed to begin with one and it must not reach the
/// parser (BurntSushi/toml-test `valid/utf8-bom-01`, `-02`). The token is
/// a `#SP`, which is in the IGNORE set, so the parser never sees it.
/// Restricted to scalar position 0, so a BOM anywhere else stays an error.
pub(crate) fn bom_matcher(
    lexer: &mut Lexer<'_>,
    _rule: &mut Rule,
    _context: &mut Context,
) -> Option<Token> {
    let point = lexer.point();
    if 0 != point.site.pos || !lexer.source().starts_with(BOM) {
        return None;
    }
    let token = lexer.token("#SP", TIN_SP, Value::Undefined, BOM.to_string(), point);
    lexer.advance_chars(1);
    Some(token)
}
