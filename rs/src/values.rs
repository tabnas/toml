// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

//! The TOML date / time value, and how it rides on a `tabnas::Value`.

use tabnas::{Text, Value};

/// The four TOML date and time kinds, in the spelling the TypeScript port
/// attaches to a JavaScript `Date` as `__toml__.kind` and the Go port
/// stores in `TomlTime.Kind`.
pub const OFFSET_DATE_TIME: &str = "offset-date-time";
/// See [`OFFSET_DATE_TIME`].
pub const LOCAL_DATE_TIME: &str = "local-date-time";
/// See [`OFFSET_DATE_TIME`].
pub const LOCAL_DATE: &str = "local-date";
/// See [`OFFSET_DATE_TIME`].
pub const LOCAL_TIME: &str = "local-time";

/// A TOML date, time or date-time, as this port carries it.
///
/// TypeScript produces a JavaScript `Date` with a `__toml__` property and
/// Go produces a `*TomlTime`; neither shape exists here, because
/// [`tabnas::Value`] is a closed enum with no room for a host type. The
/// value therefore travels as a [`Value::Text`], whose `string` is the
/// source text and whose `quote` is the kind. `Text` serializes and
/// converts to JSON as its `string`, so a parsed document renders the
/// date as it was written, which is what the TypeScript `Date` does under
/// `JSON.stringify` and closer to it than the Go struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlTime {
    /// One of the four kind constants above.
    pub kind: String,
    /// The original source text of the value.
    pub src: String,
}

impl TomlTime {
    /// Build one from its two parts.
    pub fn new(kind: impl Into<String>, src: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            src: src.into(),
        }
    }

    /// The engine value this rides on.
    pub fn to_value(&self) -> Value {
        Value::Text(Text {
            quote: self.kind.clone(),
            string: self.src.clone(),
        })
    }
}

/// Read a [`TomlTime`] back off a parsed value, or `None` when the value
/// is not one. The kind is checked against the four constants, so an
/// ordinary `Text` from some other plugin is not mistaken for a date.
pub fn toml_time(value: &Value) -> Option<TomlTime> {
    let Value::Text(text) = value else {
        return None;
    };
    matches!(
        text.quote.as_str(),
        OFFSET_DATE_TIME | LOCAL_DATE_TIME | LOCAL_DATE | LOCAL_TIME
    )
    .then(|| TomlTime::new(text.quote.clone(), text.string.clone()))
}

/// Resolve the grammar's `@isodate-val` reference: decide from the regexp
/// capture groups whether the value is a local or offset date, and whether
/// it carries a time.
///
/// Kept live for the same reason both other ports keep it: the grammar
/// text declares `val: '@isodate-val'`, so the reference has to resolve
/// when the document is installed. The context-aware matcher in
/// `datematcher` is what actually runs.
pub fn isodate_val(groups: &[String]) -> Value {
    isodate_time(groups).to_value()
}

/// The [`TomlTime`] `@isodate-val` denotes. Shared with the matcher so the
/// two cannot describe the same match differently.
pub fn isodate_time(groups: &[String]) -> TomlTime {
    // Group 4 is the zone suffix (`Z` or `+hh:mm`); present means offset.
    let offset = groups.get(4).is_some_and(|group| !group.is_empty());
    // Group 1 is the `Thh:mm...` part; present means the value has a time.
    let timed = groups.get(1).is_some_and(|group| !group.is_empty());
    let kind = match (offset, timed) {
        (true, true) => OFFSET_DATE_TIME,
        (true, false) => "offset-date",
        (false, true) => LOCAL_DATE_TIME,
        (false, false) => LOCAL_DATE,
    };
    TomlTime::new(kind, groups.first().cloned().unwrap_or_default())
}

/// Resolve the grammar's `@localtime-val` reference.
pub fn localtime_val(groups: &[String]) -> Value {
    localtime_time(groups).to_value()
}

/// The [`TomlTime`] `@localtime-val` denotes.
pub fn localtime_time(groups: &[String]) -> TomlTime {
    TomlTime::new(LOCAL_TIME, groups.first().cloned().unwrap_or_default())
}
