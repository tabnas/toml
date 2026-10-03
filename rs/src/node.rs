// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

//! Table nodes as PATHS into a shared cell, and the cursor that keeps a
//! dotted header, and a dotted key, linear in its length.
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
//! value down to the table. A write walks the path from the cell, so it
//! always lands in the real tree however the `Arc`s happen to be shared.
//!
//! Two things keep that from costing time in proportion to the path on
//! every segment of a header, which it used to (tabnas/toml#81: a header
//! of 10,000 segments took minutes, where the other two ports take
//! milliseconds):
//!
//! - A rule's path is a [`Path`], two numbers naming a prefix of a buffer
//!   in the parse's path registry, which lives in the context's `u` bag.
//!   A buffer only ever grows, so every prefix stays valid, and extending
//!   the newest path in a buffer appends to it in place. A path used to be
//!   a full copy of its segments in the rule's `u` bag, rebuilt on every
//!   segment, and the engine keeps every rule a replace loop passes
//!   through, so a header of n segments held n paths of up to n segments.
//! - A header, and a dotted key, move through the tree with a [`Cursor`]
//!   instead of walking from the cell's root once or twice per segment.
//!   See there for how it stays exact.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;
use std::sync::Arc;

use indexmap::IndexMap;
use tabnas::{ActionError, Context, Rule, RuleSnapshot, Value};

/// The cell a rule's table path is relative to.
pub(crate) type Cell = Rc<RefCell<Value>>;

/// Where a rule's table path is kept in its `u` bag. `u` is per-rule and
/// merged key by key, so an alternate's own `u: { ... }` does not disturb
/// it.
pub(crate) const PATH_KEY: &str = "toml_path";

/// Where the parse keeps its path buffers, in the context's `u` bag.
const PATHS_KEY: &str = "toml_paths";

/// Where the cursors parked between two segments wait, in the context's
/// `u` bag: a stack, the cursor parked last on top. A header parks one at
/// a time, but a dotted key's value can hold an inline table whose own
/// dotted keys park a cursor of their own before the outer key is done,
/// so the outer one waits underneath until the inner ones have finished.
const PARKED_KEY: &str = "toml_parked";

/// Where the parse keeps the tables a header's prefix created and no header
/// has yet defined, in the context's `u` bag: the only existing tables a
/// header may define. A table is told by its PATH, not by its node, which
/// `Arc::make_mut` may copy under a later write, and a path is looked up in
/// time independent of its length, as a header's segments are walked, or a
/// long header would be quadratic again (tabnas/toml#81). So every position
/// the cursor has stood on is a numbered node of a trie over the tree's
/// paths, kept flat in one map: the entry for a child is keyed by its
/// parent's number and the segment (`"7:name"`, `"7:0"`), and holds the
/// child's number and whether the table there is implicit. The cursor
/// carries its node's number along with its path. Kept on the context
/// rather than in the tree, so that a table's history never leaves a mark
/// on the value a reader gets back.
const IMPLICIT_KEY: &str = "toml_implicit";

/// Where the parse keeps the address of the document's own cell, in the
/// context's `u` bag. A header's prefix marks tables in that tree only. A
/// table body or an inline table is a cell of its own, whose paths are
/// numbered from the same root node of the trie, so a cursor on any other
/// cell must leave the marks alone: `[a.b]`, then `[c]` with `a.x = 1` and
/// `a.y = 2`, walks `c`'s own `a`, and the implicit `a` at the top of the
/// document has the same trie node.
const ROOT_KEY: &str = "toml_root";

/// A table path: the first `len` segments of buffer `id` in the parse's
/// path registry. A segment is a key (a string) into a table, or a
/// position (a number) into an array of tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Path {
    id: usize,
    len: usize,
}

impl Path {
    /// The cell's own value.
    pub(crate) const ROOT: Path = Path { id: 0, len: 0 };

    /// How many segments the path has: the tables and array positions it
    /// descends through from the cell's value.
    pub(crate) fn len(self) -> usize {
        self.len
    }
}

#[allow(clippy::cast_precision_loss)]
fn number(count: usize) -> Value {
    Value::Number(count as f64)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn count(value: Option<&Value>) -> Option<usize> {
    match value {
        Some(Value::Number(number)) if *number >= 0.0 && 0.0 == number.fract() => {
            Some(*number as usize)
        }
        _ => None,
    }
}

fn path_from(value: Option<&Value>) -> Path {
    let Some(Value::Array(parts)) = value else {
        return Path::ROOT;
    };
    match (count(parts.first()), count(parts.get(1))) {
        (Some(id), Some(len)) if 0 < len => Path { id, len },
        _ => Path::ROOT,
    }
}

/// This rule's own table path.
pub(crate) fn path_of(rule: &Rule) -> Path {
    path_from(rule.u.get(PATH_KEY))
}

/// Another rule's table path, read off the snapshot the engine handed over.
pub(crate) fn snapshot_path(rule: Option<&Rc<RuleSnapshot>>) -> Path {
    rule.map_or(Path::ROOT, |rule| path_from(rule.u.get(PATH_KEY)))
}

/// Record this rule's table path.
pub(crate) fn set_path(rule: &mut Rule, path: Path) {
    let value = Value::Array(Arc::new(vec![number(path.id), number(path.len)]));
    rule.u_mut().insert(PATH_KEY.to_string(), value);
}

/// The parse's path buffers, installed on first use. Nothing else holds a
/// handle on them, so `Arc::make_mut` here never copies.
fn buffers_mut(context: &mut Context) -> &mut Vec<Value> {
    if !matches!(context.u.get(PATHS_KEY), Some(Value::Array(_))) {
        context
            .u
            .insert(PATHS_KEY.to_string(), Value::Array(Arc::new(Vec::new())));
    }
    match context.u.get_mut(PATHS_KEY) {
        Some(Value::Array(buffers)) => Arc::make_mut(buffers),
        _ => unreachable!("the path registry was installed just above"),
    }
}

/// The segments of `path`, borrowed from the registry.
fn segments(context: &Context, path: Path) -> &[Value] {
    if 0 == path.len {
        return &[];
    }
    let Some(Value::Array(buffers)) = context.u.get(PATHS_KEY) else {
        return &[];
    };
    match buffers.get(path.id) {
        Some(Value::Array(segments)) => segments.get(..path.len).unwrap_or(&[]),
        _ => &[],
    }
}

/// `path` with one more segment. The newest path in a buffer grows in
/// place; any other path is copied into a buffer of its own first, so the
/// prefixes other rules hold never change.
fn extend(context: &mut Context, path: Path, segment: Value) -> Path {
    let buffers = buffers_mut(context);
    let prefix = if 0 == path.len {
        Vec::new()
    } else {
        match buffers.get_mut(path.id) {
            Some(Value::Array(segments)) if segments.len() == path.len => {
                Arc::make_mut(segments).push(segment);
                return Path {
                    id: path.id,
                    len: path.len + 1,
                };
            }
            Some(Value::Array(segments)) => segments
                .get(..path.len)
                .map(<[Value]>::to_vec)
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    };
    let mut segments = prefix;
    segments.push(segment);
    let len = segments.len();
    buffers.push(Value::Array(Arc::new(segments)));
    Path {
        id: buffers.len() - 1,
        len,
    }
}

/// A fresh, prototype-free table node. TOML allocates every map and table
/// itself rather than leaning on the core's allocator, so it follows the
/// core's convention too: an ordinary map with ordinary keys, which is
/// what makes `__proto__` an ordinary key here as it is in the other
/// ports.
pub(crate) fn new_map() -> Value {
    Value::Object(Arc::new(IndexMap::new()))
}

/// A segment as a position into an array.
fn position(segment: &Value) -> Option<usize> {
    count(Some(segment))
}

/// Walk a path for reading. A loop rather than recursion: a header as
/// long as the input must not be walked with the call stack.
fn at<'v>(mut value: &'v Value, path: &[Value]) -> Option<&'v Value> {
    for segment in path {
        value = match (value, segment) {
            (Value::Object(map), Value::String(key)) => map.get(key)?,
            (Value::Array(list), Value::Number(_)) => list.get(position(segment)?)?,
            _ => return None,
        };
    }
    Some(value)
}

/// Walk a path for writing. Every step goes through `Arc::make_mut`, so
/// the walk owns the spine it descends and the write lands in the tree the
/// cell holds.
fn at_mut<'v>(mut value: &'v mut Value, path: &[Value]) -> Option<&'v mut Value> {
    for segment in path {
        value = match (value, segment) {
            (Value::Object(map), Value::String(key)) => Arc::make_mut(map).get_mut(key)?,
            (Value::Array(list), Value::Number(_)) => {
                Arc::make_mut(list).get_mut(position(segment)?)?
            }
            _ => return None,
        };
    }
    Some(value)
}

/// Put `value` at `last` in the container at `parent`, CREATING that final
/// key when the container does not have it yet. Only the final segment is
/// created: a parent that does not exist makes this a no-op, and every
/// caller builds the parent first.
fn write(cell: &Cell, parent: &[Value], last: &Value, value: Value) {
    let mut root = cell.borrow_mut();
    let Some(container) = at_mut(&mut root, parent) else {
        return;
    };
    match (container, last) {
        (Value::Object(map), Value::String(key)) => {
            Arc::make_mut(map).insert(key.clone(), value);
        }
        (Value::Array(list), Value::Number(_)) => {
            let Some(index) = position(last) else {
                return;
            };
            let list = Arc::make_mut(list);
            match index.cmp(&list.len()) {
                Ordering::Less => list[index] = value,
                Ordering::Equal => list.push(value),
                // Past the end: the parent does not have the slot, and
                // `write` creates only its final segment.
                Ordering::Greater => {}
            }
        }
        _ => {}
    }
}

/// Put `value` under `key` in the table at `path`, the write
/// `r.node[key] = r.child.node` makes in the canonical grammar.
pub(crate) fn write_at(context: &mut Context, cell: &Cell, path: Path, key: String, value: Value) {
    settle(context, cell);
    write(cell, segments(context, path), &Value::String(key), value);
}

/// Copy every entry of `source` into the map at `path`, the write
/// `Object.assign(r.node, r.child.node)` makes in the canonical grammar.
pub(crate) fn merge_into(context: &mut Context, cell: &Cell, path: Path, source: &Value) {
    settle(context, cell);
    let Value::Object(entries) = source else {
        return;
    };
    let entries: Vec<(String, Value)> = entries
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if let Some(Value::Object(target)) = at_mut(&mut cell.borrow_mut(), segments(context, path)) {
        let target = Arc::make_mut(target);
        for (key, value) in entries {
            target.insert(key, value);
        }
    }
}

/// What a segment asks of the table under its key.
///
/// A header like `[fruit.variety]` walks THROUGH `fruit` and DEFINES
/// `variety`, and a dotted key like `fruit.variety = 1` walks through
/// `fruit` as well. The three positions have different rules. Descending
/// through an array of tables is how `[[x]]` followed by `[x.y]` works, and
/// four valid corpus documents rely on it; landing on one as the thing
/// being defined is `[x]` trying to redefine `[[x]]`: invalid TOML. A table
/// a header walks through and finds missing is created implicitly, and TOML
/// lets one later header define it (`[a.b]` then `[a]`); a table a header
/// has DEFINED, a dotted key has created or walked through, or an inline
/// table has made may not be defined by a header again (`[a]` twice,
/// `a.b = 1` then `[a]`, `a = {}` then `[a]`).
///
/// The grammar already separates the positions: `#DOT`-terminated segments
/// of a header are intermediate, `#CS`-terminated ones are final, and a
/// dotted key's segments are the `dive` rule's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// A header's leading segment.
    Descend,
    /// A header's last segment.
    Define,
    /// A dotted key's leading segment.
    Dive,
}

fn conflict(key: &str, why: &str) -> ActionError {
    // The engine renders the code through `options.error`, which this
    // plugin registers, so the message a reader sees is the one the
    // TypeScript port writes.
    ActionError::new("toml_key_conflict", format!("cannot define {key}, {why}"))
}

fn describe(value: &Value) -> String {
    format!("it already has the value {}", value.to_json())
}

/// The refusal to give `key` a second value when it already holds
/// `existing`: `a = 1` then `a = 2`, `a.b = 1` then `a.b = 2`,
/// `{b = 1, b = 2}`. TOML allows a key one value, and the base grammar's
/// duplicate-key rule (last wins, tables merged) is not it.
pub(crate) fn redefined(key: &str, existing: &Value) -> ActionError {
    conflict(key, &describe(existing))
}

/// The value under `key` in the table at `path`, when the table has one.
pub(crate) fn existing_at(
    context: &mut Context,
    cell: &Cell,
    path: Path,
    key: &str,
) -> Option<Value> {
    settle(context, cell);
    let root = cell.borrow();
    match at(&root, segments(context, path)) {
        Some(Value::Object(map)) => match map.get(key) {
            None | Some(Value::Undefined | Value::Null) => None,
            Some(existing) => Some(existing.clone()),
        },
        _ => None,
    }
}

/// The trie over the tree's paths (see [`IMPLICIT_KEY`]), installed on
/// first use. Nothing else holds a handle on it, so `Arc::make_mut` here
/// never copies.
fn trie_mut(context: &mut Context) -> &mut IndexMap<String, Value> {
    if !matches!(context.u.get(IMPLICIT_KEY), Some(Value::Object(_))) {
        context.u.insert(
            IMPLICIT_KEY.to_string(),
            Value::Object(Arc::new(IndexMap::new())),
        );
    }
    match context.u.get_mut(IMPLICIT_KEY) {
        Some(Value::Object(trie)) => Arc::make_mut(trie),
        _ => unreachable!("the trie was installed just above"),
    }
}

/// The key of `segment`'s entry under trie node `parent`: the parent's
/// number, a colon, and the segment. The number ends at the first colon,
/// so no two (parent, segment) pairs share a key, whatever the segment.
fn trie_key(parent: usize, segment: &Value) -> String {
    match segment {
        Value::String(key) => format!("{parent}:{key}"),
        other => format!("{parent}:{}", position(other).unwrap_or(0)),
    }
}

/// The entry for `segment` under trie node `parent`, made (and not
/// implicit) when missing: the child's number, and whether the table there
/// is implicit.
fn trie_entry(context: &mut Context, parent: usize, segment: &Value) -> (usize, bool) {
    let key = trie_key(parent, segment);
    let trie = trie_mut(context);
    if let Some(Value::Array(entry)) = trie.get(&key) {
        if let Some(id) = count(entry.first()) {
            return (id, matches!(entry.get(1), Some(Value::Bool(true))));
        }
    }
    let id = trie.len() + 1;
    trie.insert(
        key,
        Value::Array(Arc::new(vec![number(id), Value::Bool(false)])),
    );
    (id, false)
}

/// The trie node for `segment` under `parent`.
fn trie_child(context: &mut Context, parent: usize, segment: &Value) -> usize {
    trie_entry(context, parent, segment).0
}

/// Record `cell` as the document's own cell (see [`ROOT_KEY`]). Kept as two
/// halves, as a parked cursor keeps its cell, so the address survives the
/// trip through an `f64`.
pub(crate) fn set_root(context: &mut Context, cell: &Cell) {
    let id = cell_id(cell) as u64;
    let halves = Value::Array(Arc::new(vec![
        number((id >> 32) as usize),
        number((id & 0xffff_ffff) as usize),
    ]));
    context.u.insert(ROOT_KEY.to_string(), halves);
}

/// Whether `cell`, an address as [`cell_id`] gives it, is the document's
/// own cell.
fn is_root(context: &Context, cell: usize) -> bool {
    let Some(Value::Array(halves)) = context.u.get(ROOT_KEY) else {
        return false;
    };
    match (count(halves.first()), count(halves.get(1))) {
        #[allow(clippy::cast_possible_truncation)]
        (Some(high), Some(low)) => cell == (((high as u64) << 32) | low as u64) as usize,
        _ => false,
    }
}

/// Record whether the table at `segment` under trie node `parent` is
/// implicit, and answer whether it was.
fn set_implicit(context: &mut Context, parent: usize, segment: &Value, implicit: bool) -> bool {
    let (id, was) = trie_entry(context, parent, segment);
    let key = trie_key(parent, segment);
    trie_mut(context).insert(
        key,
        Value::Array(Arc::new(vec![number(id), Value::Bool(implicit)])),
    );
    was
}

/// What a header has created and not yet written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fresh {
    /// An empty table.
    Table,
    /// An empty array of tables.
    Array,
}

impl Fresh {
    fn empty(self) -> Value {
        match self {
            Fresh::Table => new_map(),
            Fresh::Array => Value::Array(Arc::new(Vec::new())),
        }
    }
}

/// What sits at a cursor's position.
enum Here {
    /// A node already in the tree. The handle is only ever read through,
    /// and it is let go before anything writes to the tree, so it never
    /// makes a write copy the node.
    Node(Value),
    /// A container the cursor has created and not yet written: empty, so
    /// every key looked up in it is absent.
    Fresh(Fresh),
    /// Nothing, and nothing can be created here: the parent does not take
    /// this kind of segment, so a write here does nothing, as `write`
    /// makes it.
    Absent,
}

/// The containers a header has created and not yet written: one at each
/// position of its path from `base` to `end`, the deepest an empty
/// `last`. The one at `base` goes into a node already in the tree, each
/// of the others into the one before it.
#[derive(Debug, Clone, Copy)]
struct Pending {
    base: usize,
    end: usize,
    last: Fresh,
}

/// A cursor that moves a table header, or a dotted key, through the tree
/// one segment at a time, so one of n segments costs O(n) rather than
/// O(n^2).
///
/// Every segment of a header looks a key up in the node the previous
/// segment reached, and in the canonical grammar it is handed that node
/// as `r.prev.node`. A path re-finds it from the cell's root, and with
/// `Arc` containers there is no handle on a nested node that can be
/// written through, so the cursor does two things instead:
///
/// - While the header walks through tables that already exist, it holds
///   a read-only handle on the node it has reached. Nothing writes to the
///   tree between two segments of a header, and the handle is let go
///   before anything does.
/// - Once a segment creates a table, every later one lands in something
///   just created, which is empty. So nothing is looked up, nothing can
///   conflict, and the created containers are recorded as [`Pending`] and
///   written in ONE walk when the header ends: when its body follows
///   (`done` in `refs::conclude`), when an action fails (so a diagnostic
///   is raised over the same tree as it always was), or as soon as
///   anything else reads or writes the tree ([`settle`]). Each table is
///   created in the same container, under the same key, in the same
///   order, so the document is the same; only the moment the containers
///   reach the tree moves, from each segment to the end of the header.
///
/// No parse result can see that moment, with one exception: a parse that
/// RECOVERS from errors reads the whole tree after every action, as its
/// partial value. So when `parse.recover.enabled` is set, [`Cursor::park`]
/// writes everything out at the end of each action instead, and the tree
/// after every action is exactly what it was when each segment wrote
/// straight into it. That costs the old O(path) per segment, but only in
/// that mode, where taking the partial value already copies the spine of
/// the tree on every write.
pub(crate) struct Cursor {
    /// The address of the cell, telling a cursor parked on this cell from
    /// one parked on another.
    cell: usize,
    path: Path,
    /// The trie node of the position, see [`IMPLICIT_KEY`]: moved with the
    /// path, one entry per segment.
    trie: usize,
    here: Here,
    pending: Option<Pending>,
}

fn cell_id(cell: &Cell) -> usize {
    Rc::as_ptr(cell).addr()
}

impl Cursor {
    /// The cursor the previous segment of this header or key parked, when
    /// it is on `path`; otherwise one found by walking. A cursor parked on
    /// another cell belongs to a key further out, whose value this one is
    /// inside, and is left where it is.
    pub(crate) fn resume(context: &mut Context, cell: &Cell, path: Path) -> Cursor {
        if let Some(parked) = take_parked(context) {
            if parked.cell != cell_id(cell) {
                parked.store(context);
            } else if parked.path == path {
                return parked;
            } else {
                parked.finish(context, cell);
            }
        }
        Cursor::find(context, cell, path)
    }

    fn find(context: &mut Context, cell: &Cell, path: Path) -> Cursor {
        let parts = segments(context, path).to_vec();
        let here = at(&cell.borrow(), &parts).map_or(Here::Absent, |node| Here::Node(node.clone()));
        let trie = parts
            .iter()
            .fold(0, |trie, segment| trie_child(context, trie, segment));
        Cursor {
            cell: cell_id(cell),
            path,
            trie,
            here,
            pending: None,
        }
    }

    /// Where the cursor stands.
    pub(crate) fn path(&self) -> Path {
        self.path
    }

    /// The value at the cursor, as reading the tree there finds it.
    pub(crate) fn value(&self) -> Value {
        match &self.here {
            Here::Node(node) => node.clone(),
            Here::Fresh(fresh) => fresh.empty(),
            Here::Absent => Value::Undefined,
        }
    }

    /// The canonical `Array.isArray(node)`.
    pub(crate) fn is_list(&self) -> bool {
        matches!(
            self.here,
            Here::Node(Value::Array(_)) | Here::Fresh(Fresh::Array)
        )
    }

    /// The value under `key` in the table at the cursor.
    fn child(&self, key: &str) -> Option<&Value> {
        match &self.here {
            Here::Node(Value::Object(map)) => map.get(key),
            _ => None,
        }
    }

    /// Whether the table at the cursor holds an array under `key`.
    pub(crate) fn has_list(&self, key: &str) -> bool {
        matches!(self.child(key), Some(Value::Array(_)))
    }

    /// Move to whatever is under `key`.
    pub(crate) fn enter(&mut self, context: &mut Context, key: &str) {
        let here = self
            .child(key)
            .map_or(Here::Absent, |node| Here::Node(node.clone()));
        let segment = Value::String(key.to_string());
        self.trie = trie_child(context, self.trie, &segment);
        self.path = extend(context, self.path, segment);
        self.here = here;
    }

    /// Create an empty `fresh` container at `segment` under the cursor,
    /// and move to it. The write it stands for is recorded, not made.
    fn create(&mut self, context: &mut Context, segment: Value, fresh: Fresh) {
        // The same test `write` makes: a table takes a key, an array a
        // position no further than its end, and nothing else takes
        // anything.
        let takes = match (&self.here, &segment) {
            (Here::Node(Value::Object(_)) | Here::Fresh(Fresh::Table), Value::String(_)) => true,
            (Here::Node(Value::Array(list)), Value::Number(_)) => {
                position(&segment).is_some_and(|index| index <= list.len())
            }
            (Here::Fresh(Fresh::Array), Value::Number(_)) => Some(0) == position(&segment),
            _ => false,
        };
        let from = self.path.len;
        self.trie = trie_child(context, self.trie, &segment);
        self.path = extend(context, self.path, segment);
        if !takes {
            self.here = Here::Absent;
            return;
        }
        // Only an empty container can be under one just created, so a
        // header that has created anything is still at the end of what it
        // created, and this extends it.
        debug_assert!(self.pending.is_none_or(|pending| pending.end == from));
        let base = self.pending.map_or(from, |pending| pending.base);
        self.pending = Some(Pending {
            base,
            end: self.path.len,
            last: fresh,
        });
        self.here = Here::Fresh(fresh);
    }

    /// The canonical `tableAt(node, key, …)`: the table under `key`, or a
    /// DIAGNOSED refusal to descend into something that is not one, or to
    /// define a table a second time.
    ///
    /// TOML forbids redefining a key, so `a = {b = 1, b.c = 2}` and `a = 1`
    /// followed by `[a.b]` are invalid documents. Answering with a real
    /// code rather than walking into the scalar is what makes the rejection
    /// a conformant one.
    pub(crate) fn table_at(
        &mut self,
        context: &mut Context,
        key: &str,
        reach: Reach,
    ) -> Result<(), ActionError> {
        match self.child(key) {
            None | Some(Value::Undefined | Value::Null) => {
                let segment = Value::String(key.to_string());
                if Reach::Descend == reach {
                    // A header's prefix: the one kind of table a later
                    // header may define.
                    set_implicit(context, self.trie, &segment, true);
                }
                self.create(context, segment, Fresh::Table);
                Ok(())
            }
            Some(Value::Array(_)) => {
                if Reach::Define != reach {
                    self.enter(context, key);
                    return Ok(());
                }
                // `[[fruit.variety]]` then `[fruit.variety]`. Accepting
                // this destroys data whichever way it is resolved: one port
                // kept the array and dropped the second table, the other
                // replaced the array and dropped the first.
                Err(conflict(key, "it is already an array of tables"))
            }
            // An existing TABLE passes when walked through. A header may
            // DEFINE it only if a header's prefix made it and no header has
            // defined it yet: `[a]` after `[a.b]` is that one case, and
            // `[a]` after `[a]`, after `a.b = 1` or after `a = {}` is a key
            // defined twice. Every port used to let all of those through.
            //
            // A dotted key that walks through a table defines it, as it
            // defines every table it creates, so the table stops being
            // implicit and no header defines it afterwards. Only on the
            // document's own cell, see [`ROOT_KEY`], which a dotted key
            // walks only before the first header, when nothing is implicit
            // yet, so no document reaches this today; it keeps the rule true
            // without leaning on that. The document that showed the rule
            // missing, `[x.a.b]`, `[x]` with `a.c = 1`, then `[x.a]`, was
            // accepted here for another reason: the marks are kept by path,
            // and the body's merge replaced the table at `x.a` under the
            // mark `[x.a.b]` had left. `refs::body_conflict` refuses it at
            // `a.c` now.
            Some(Value::Object(_) | Value::MapRef(_)) => {
                let segment = Value::String(key.to_string());
                match reach {
                    Reach::Define => {
                        if !set_implicit(context, self.trie, &segment, false) {
                            return Err(conflict(key, "it is already defined"));
                        }
                    }
                    Reach::Dive => {
                        if is_root(context, self.cell) {
                            set_implicit(context, self.trie, &segment, false);
                        }
                    }
                    Reach::Descend => {}
                }
                self.enter(context, key);
                Ok(())
            }
            Some(other) => Err(conflict(key, &describe(other))),
        }
    }

    /// The array of tables under `key`, or a DIAGNOSED refusal to append to
    /// something that is not an array.
    pub(crate) fn array_at(&mut self, context: &mut Context, key: &str) -> Result<(), ActionError> {
        match self.child(key) {
            Some(Value::Array(_)) => {
                self.enter(context, key);
                Ok(())
            }
            None | Some(Value::Undefined | Value::Null) => {
                self.create(context, Value::String(key.to_string()), Fresh::Array);
                Ok(())
            }
            Some(other) => Err(conflict(key, &describe(other))),
        }
    }

    /// Move to the LAST table of the array at the cursor, growing the array
    /// by an empty table when it has none. The canonical
    /// `last = last ? last : (arr.push(node()), arr[arr.length - 1])`.
    pub(crate) fn last_of_list(&mut self, context: &mut Context) {
        let last = match &self.here {
            Here::Node(Value::Array(list)) => list
                .len()
                .checked_sub(1)
                .map(|index| (index, list[index].clone())),
            _ => None,
        };
        match last {
            Some((index, node)) => {
                self.trie = trie_child(context, self.trie, &number(index));
                self.path = extend(context, self.path, number(index));
                self.here = Here::Node(node);
            }
            None => self.create(context, number(0), Fresh::Table),
        }
    }

    /// Append an empty table to the array at the cursor, and move to it:
    /// the canonical `r.prev.node.push(node())`.
    pub(crate) fn push_table(&mut self, context: &mut Context) {
        let index = match &self.here {
            Here::Node(Value::Array(list)) => list.len(),
            _ => 0,
        };
        self.create(context, number(index), Fresh::Table);
    }

    /// Write what this header has created into the tree, in one walk, and
    /// let go of the cursor.
    pub(crate) fn finish(self, context: &Context, cell: &Cell) {
        let Cursor {
            path,
            here,
            pending,
            ..
        } = self;
        // Let go of any handle on the tree before writing to it.
        drop(here);
        let Some(pending) = pending else {
            return;
        };
        let segments = segments(context, path);
        let (Some(parent), Some(slot), Some(below)) = (
            segments.get(..pending.base),
            segments.get(pending.base),
            segments.get(pending.base + 1..pending.end),
        ) else {
            return;
        };
        // Built from the deepest container up: each segment under the one
        // at `base` is a key into a fresh table or the first position of a
        // fresh array.
        let mut value = pending.last.empty();
        for segment in below.iter().rev() {
            value = match segment {
                Value::String(key) => {
                    let mut map = IndexMap::new();
                    map.insert(key.clone(), value);
                    Value::Object(Arc::new(map))
                }
                _ => Value::Array(Arc::new(vec![value])),
            };
        }
        write(cell, parent, slot, value);
    }

    /// Leave the cursor for the next segment of this header. See the type's
    /// documentation for why a recovering parse is written out instead.
    pub(crate) fn park(self, context: &mut Context, cell: &Cell) {
        if context.options.parse.recover.enabled {
            self.finish(context, cell);
        } else {
            self.store(context);
        }
    }

    fn store(self, context: &mut Context) {
        let (here, node) = match self.here {
            Here::Node(node) => (0, node),
            Here::Fresh(Fresh::Table) => (1, Value::Undefined),
            Here::Fresh(Fresh::Array) => (2, Value::Undefined),
            Here::Absent => (3, Value::Undefined),
        };
        // `base + 1`, so that 0 can say there is nothing pending.
        let (base, end, last) = self.pending.map_or((0, 0, 0), |pending| {
            let last = if Fresh::Array == pending.last { 2 } else { 1 };
            (pending.base + 1, pending.end, last)
        });
        let cell = self.cell as u64;
        let parked = Value::Array(Arc::new(vec![
            number((cell >> 32) as usize),
            number((cell & 0xffff_ffff) as usize),
            number(self.path.id),
            number(self.path.len),
            number(here),
            node,
            number(base),
            number(end),
            number(last),
            number(self.trie),
        ]));
        match context.u.get_mut(PARKED_KEY) {
            Some(Value::Array(stack)) => Arc::make_mut(stack).push(parked),
            _ => {
                context
                    .u
                    .insert(PARKED_KEY.to_string(), Value::Array(Arc::new(vec![parked])));
            }
        }
    }
}

/// The cursor parked last, taken out of the context.
fn take_parked(context: &mut Context) -> Option<Cursor> {
    let Some(Value::Array(stack)) = context.u.get_mut(PARKED_KEY) else {
        return None;
    };
    let Value::Array(parts) = Arc::make_mut(stack).pop()? else {
        return None;
    };
    let mut parts = Arc::try_unwrap(parts).unwrap_or_else(|shared| (*shared).clone());
    let field = |index: usize| count(parts.get(index));
    let (
        Some(high),
        Some(low),
        Some(id),
        Some(len),
        Some(here),
        Some(base),
        Some(end),
        Some(last),
        Some(trie),
    ) = (
        field(0),
        field(1),
        field(2),
        field(3),
        field(4),
        field(6),
        field(7),
        field(8),
        field(9),
    )
    else {
        return None;
    };
    let node = std::mem::replace(parts.get_mut(5)?, Value::Undefined);
    let fresh = |tag: usize| if 2 == tag { Fresh::Array } else { Fresh::Table };
    #[allow(clippy::cast_possible_truncation)]
    let cell = (((high as u64) << 32) | low as u64) as usize;
    Some(Cursor {
        cell,
        path: if 0 == len {
            Path::ROOT
        } else {
            Path { id, len }
        },
        trie,
        here: match here {
            0 => Here::Node(node),
            1 | 2 => Here::Fresh(fresh(here)),
            _ => Here::Absent,
        },
        pending: (0 < base).then(|| Pending {
            base: base - 1,
            end,
            last: fresh(last),
        }),
    })
}

/// Write out the cursor parked last, when it is parked on `cell`, before
/// anything else reads or writes that cell's tree. One parked on another
/// cell belongs to a key further out and is left where it is: its own
/// write settles it once the value inside it is done.
pub(crate) fn settle(context: &mut Context, cell: &Cell) {
    if let Some(parked) = take_parked(context) {
        if parked.cell == cell_id(cell) {
            parked.finish(context, cell);
        } else {
            parked.store(context);
        }
    }
}
