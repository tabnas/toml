// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

//! Table nodes as PATHS into a shared cell.
//!
//! The canonical TypeScript grammar hands a table rule a REFERENCE to a
//! nested object (`r.node = r.parent.node[key]`) and then lets the pairs
//! of that table be written straight into it. JavaScript objects and Go's
//! `*OrderedMap` both alias; a `tabnas::Value` does not. Its containers
//! are `Arc<IndexMap>` and `Arc<Vec>`, mutated through `Arc::make_mut`,
//! which COPIES as soon as a second handle exists, so a clone of a nested
//! map is a snapshot and writes to it never reach the document.
//!
//! So a node here is a cell plus a PATH: the `Rc<RefCell<Value>>` the rule
//! already carries, and the list of keys and indices from that cell's
//! value down to the table. Every read walks the path from the cell and
//! every write walks it again, so a write always lands in the real tree
//! however the `Arc`s happen to be shared. The paths are as deep as the
//! table nesting, which is a handful of segments.
//!
//! The path lives in the rule's `u` bag under [`PATH_KEY`], because `u` is
//! per-rule and merged key by key, so an alternate's own `u: { ... }` does
//! not disturb it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use indexmap::IndexMap;
use tabnas::{ActionError, Rule, RuleSnapshot, Value};

/// Where a rule's table path is kept in its `u` bag.
pub(crate) const PATH_KEY: &str = "toml_path";

/// One step from a cell's value towards a table node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Seg {
    /// Into a map, by key.
    Key(String),
    /// Into an array of tables, by position.
    Index(usize),
}

/// A path is stored in `u` as an array of strings and numbers, which is
/// all a `Value` can hold.
fn path_to_value(path: &[Seg]) -> Value {
    Value::Array(Arc::new(
        path.iter()
            .map(|seg| match seg {
                Seg::Key(key) => Value::String(key.clone()),
                #[allow(clippy::cast_precision_loss)]
                Seg::Index(index) => Value::Number(*index as f64),
            })
            .collect(),
    ))
}

fn value_to_path(value: Option<&Value>) -> Vec<Seg> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            Value::String(key) => Some(Seg::Key(key.clone())),
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Value::Number(index) if *index >= 0.0 => Some(Seg::Index(*index as usize)),
            _ => None,
        })
        .collect()
}

/// This rule's own table path.
pub(crate) fn path_of(rule: &Rule) -> Vec<Seg> {
    value_to_path(rule.u.get(PATH_KEY))
}

/// Another rule's table path, read off the snapshot the engine handed over.
pub(crate) fn snapshot_path(rule: Option<&Rc<RuleSnapshot>>) -> Vec<Seg> {
    rule.map_or_else(Vec::new, |rule| value_to_path(rule.u.get(PATH_KEY)))
}

/// Record this rule's table path.
pub(crate) fn set_path(rule: &mut Rule, path: &[Seg]) {
    let value = path_to_value(path);
    rule.u_mut().insert(PATH_KEY.to_string(), value);
}

/// A fresh, prototype-free table node. TOML allocates every map and table
/// itself rather than leaning on the core's allocator, so it follows the
/// core's convention too: an ordinary map with ordinary keys, which is
/// what makes `__proto__` an ordinary key here as it is in the other
/// ports.
pub(crate) fn new_map() -> Value {
    Value::Object(Arc::new(IndexMap::new()))
}

/// Walk a path for reading.
fn at<'v>(value: &'v Value, path: &[Seg]) -> Option<&'v Value> {
    match path.split_first() {
        None => Some(value),
        Some((seg, rest)) => {
            let child = match (value, seg) {
                (Value::Object(map), Seg::Key(key)) => map.get(key)?,
                (Value::Array(list), Seg::Index(index)) => list.get(*index)?,
                _ => return None,
            };
            at(child, rest)
        }
    }
}

/// Walk a path for writing. Every step goes through `Arc::make_mut`, so
/// the walk owns the spine it descends and the write lands in the tree the
/// cell holds.
fn at_mut<'v>(value: &'v mut Value, path: &[Seg]) -> Option<&'v mut Value> {
    match path.split_first() {
        None => Some(value),
        Some((seg, rest)) => {
            let child = match (value, seg) {
                (Value::Object(map), Seg::Key(key)) => Arc::make_mut(map).get_mut(key)?,
                (Value::Array(list), Seg::Index(index)) => Arc::make_mut(list).get_mut(*index)?,
                _ => return None,
            };
            at_mut(child, rest)
        }
    }
}

/// A clone of the value at `path`, or `None` when nothing is there.
pub(crate) fn read(cell: &Rc<RefCell<Value>>, path: &[Seg]) -> Option<Value> {
    at(&cell.borrow(), path).cloned()
}

/// Whether the value at `path` is an array of tables.
pub(crate) fn is_list(cell: &Rc<RefCell<Value>>, path: &[Seg]) -> bool {
    matches!(read(cell, path), Some(Value::Array(_)))
}

/// How many entries the array at `path` holds, or 0 when it is not one.
pub(crate) fn list_len(cell: &Rc<RefCell<Value>>, path: &[Seg]) -> usize {
    match at(&cell.borrow(), path) {
        Some(Value::Array(list)) => list.len(),
        _ => 0,
    }
}

/// Put `value` at `path`, CREATING the final key when the container does
/// not have it yet. Only the final segment is created: a path whose parent
/// does not exist is a no-op, and every caller here builds the parent
/// first.
pub(crate) fn write(cell: &Rc<RefCell<Value>>, path: &[Seg], value: Value) {
    let Some((last, parent)) = path.split_last() else {
        *cell.borrow_mut() = value;
        return;
    };
    let mut root = cell.borrow_mut();
    let Some(container) = at_mut(&mut root, parent) else {
        return;
    };
    match (container, last) {
        (Value::Object(map), Seg::Key(key)) => {
            Arc::make_mut(map).insert(key.clone(), value);
        }
        (Value::Array(list), Seg::Index(index)) => {
            let list = Arc::make_mut(list);
            if *index < list.len() {
                list[*index] = value;
            } else if *index == list.len() {
                list.push(value);
            }
        }
        _ => {}
    }
}

/// Append `value` to the array at `path`, answering its new index.
pub(crate) fn push(cell: &Rc<RefCell<Value>>, path: &[Seg], value: Value) -> Option<usize> {
    match at_mut(&mut cell.borrow_mut(), path) {
        Some(Value::Array(list)) => {
            let list = Arc::make_mut(list);
            list.push(value);
            Some(list.len() - 1)
        }
        _ => None,
    }
}

/// Copy every entry of `source` into the map at `path`, the write
/// `Object.assign(r.node, r.child.node)` makes in the canonical grammar.
pub(crate) fn merge_into(cell: &Rc<RefCell<Value>>, path: &[Seg], source: &Value) {
    let Value::Object(entries) = source else {
        return;
    };
    let entries: Vec<(String, Value)> = entries
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if let Some(Value::Object(target)) = at_mut(&mut cell.borrow_mut(), path) {
        let target = Arc::make_mut(target);
        for (key, value) in entries {
            target.insert(key, value);
        }
    }
}

/// Descending THROUGH an array of tables is how `[[x]]` followed by
/// `[x.y]` works; landing ON one as the thing being defined is `[x]`
/// trying to redefine `[[x]]`, which is invalid TOML. The grammar already
/// separates the two: `#DOT`-terminated segments are intermediate,
/// `#CS`-terminated ones are final.
pub(crate) const DESCEND: bool = true;
/// See [`DESCEND`].
pub(crate) const DEFINE: bool = false;

fn conflict(key: &str, why: &str) -> ActionError {
    // The engine renders the code through `options.error`, which this
    // plugin registers, so the message a reader sees is the one the
    // TypeScript port writes.
    ActionError::new("toml_key_conflict", format!("cannot define {key}, {why}"))
}

fn describe(value: &Value) -> String {
    format!("it already has the value {}", value.to_json())
}

/// The path of a table node under `parent`, or a DIAGNOSED refusal to
/// descend into something that is not one.
///
/// TOML forbids redefining a key, so `a = {b = 1, b.c = 2}` and `a = 1`
/// followed by `[a.b]` are invalid documents. Answering with a real code
/// rather than walking into the scalar is what makes the rejection a
/// conformant one.
pub(crate) fn table_at(
    cell: &Rc<RefCell<Value>>,
    parent: &[Seg],
    key: &str,
    descend: bool,
) -> Result<Vec<Seg>, ActionError> {
    let mut path = parent.to_vec();
    path.push(Seg::Key(key.to_string()));

    match read(cell, &path) {
        None | Some(Value::Undefined) | Some(Value::Null) => {
            write(cell, &path, new_map());
            Ok(path)
        }
        Some(Value::Array(_)) => {
            if descend {
                return Ok(path);
            }
            // `[[fruit.variety]]` then `[fruit.variety]`. Accepting this
            // destroys data whichever way it is resolved: one port kept
            // the array and dropped the second table, the other replaced
            // the array and dropped the first.
            Err(conflict(key, "it is already an array of tables"))
        }
        // An existing TABLE passes straight through.
        Some(Value::Object(_)) | Some(Value::MapRef(_)) => Ok(path),
        Some(other) => Err(conflict(key, &describe(&other))),
    }
}

/// The path of the array of tables under `parent`, or a DIAGNOSED refusal
/// to append to something that is not an array.
pub(crate) fn array_at(
    cell: &Rc<RefCell<Value>>,
    parent: &[Seg],
    key: &str,
) -> Result<Vec<Seg>, ActionError> {
    let mut path = parent.to_vec();
    path.push(Seg::Key(key.to_string()));

    match read(cell, &path) {
        Some(Value::Array(_)) => Ok(path),
        None | Some(Value::Undefined) | Some(Value::Null) => {
            write(cell, &path, Value::Array(Arc::new(Vec::new())));
            Ok(path)
        }
        Some(other) => Err(conflict(key, &describe(&other))),
    }
}

/// The path of the LAST table of the array at `path`, growing the array by
/// an empty table when it has none. The canonical
/// `last = last ? last : (arr.push(node()), arr[arr.length - 1])`.
pub(crate) fn last_of_list(cell: &Rc<RefCell<Value>>, path: &[Seg]) -> Vec<Seg> {
    let mut length = list_len(cell, path);
    if 0 == length {
        push(cell, path, new_map());
        length = list_len(cell, path);
    }
    let mut last = path.to_vec();
    last.push(Seg::Index(length.saturating_sub(1)));
    last
}
