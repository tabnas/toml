// Copyright (c) 2021-2026 Richard Rodger, MIT License

//! The function references the declarative grammar names with `@`.
//!
//! Registered on the instance BEFORE the grammar document is installed,
//! because the document is what looks them up. The state actions are wired
//! by the `@<rule>-<phase>` convention; the alternate actions, conditions
//! and conditional `p:` / `r:` targets are named explicitly.

use std::cell::RefCell;
use std::rc::Rc;

use tabnas::{ActionError, Context, Rule, RuleSnapshot, Tabnas, Value};

use crate::node::{
    merge_into, new_map, path_of, set_path, snapshot_path, write_at, Cell, Cursor, Path, DEFINE,
    DESCEND,
};
use crate::strmatcher::make_toml_string_matcher;
use crate::values::{isodate_val, localtime_val};

/// A matched token's value as a table key. A `#ID` or `#ST` key arrives as
/// a string; anything else falls back to the source text, as the Go
/// `tokenString` does.
fn key_of(rule: &mut Rule, context: &mut Context) -> String {
    match rule.resolve_open_value(0, context) {
        Value::String(text) => text,
        Value::Text(text) => text.string,
        _ => rule
            .o0()
            .map(|token| token.src.to_string())
            .unwrap_or_default(),
    }
}

/// The cell a rule's table path is relative to.
fn cell_of(rule: &Rule) -> Cell {
    Rc::clone(&rule.node)
}

/// The path this rule's PARENT sits at.
fn parent_path(rule: &Rule) -> Path {
    snapshot_path(rule.parent_rule.as_ref())
}

/// The path the rule this one REPLACED sat at.
fn prev_path(rule: &Rule) -> Path {
    snapshot_path(rule.prev_rule.as_ref())
}

/// Whether this header is an array of tables, `[[...]]`.
fn table_array(rule: &Rule) -> bool {
    0 < rule.n.get("table_array").copied().unwrap_or(0)
}

/// Whether a dive that replaced `previous` continues the dotted key that
/// rule began: it was reached by `r: dive` from a segment ending in a dot.
/// A dive pushed by a pair or a map begins a key, and so does one reached
/// through the close loop from a dive that ENDED a key (`dive_end`), which
/// takes the next dotted key without returning to the pair. The depth
/// guard asks the same question of a dive that has not run `@dive-bo` yet.
pub(crate) fn continues_key(previous: Option<&Rc<RuleSnapshot>>) -> bool {
    previous.is_some_and(|previous| {
        "dive" == previous.name.as_ref() && !truthy(previous.u.get("dive_end"))
    })
}

/// The end of every action that moves a table path. On success, record
/// where the header now stands, then either leave its cursor for the next
/// segment or, when `done` (the table's body follows), write out what the
/// header created. On failure, write it out before answering, so the
/// diagnostic is raised over the same tree as it always was.
fn conclude(
    rule: &mut Rule,
    context: &mut Context,
    cell: &Cell,
    cursor: Cursor,
    outcome: Result<(), ActionError>,
    done: bool,
) -> Result<(), ActionError> {
    if outcome.is_ok() {
        set_path(rule, cursor.path());
    }
    if done || outcome.is_err() {
        cursor.finish(context, cell);
    } else {
        cursor.park(context, cell);
    }
    outcome
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Undefined | Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(number)) => 0.0 != *number,
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

/// Install every `@`-named reference the grammar uses.
pub(crate) fn register(parser: &mut Tabnas) {
    register_matchers(parser);
    register_state_actions(parser);
    register_alt_actions(parser);
    register_conditions(parser);
    register_routes(parser);
}

fn register_matchers(parser: &mut Tabnas) {
    // The custom string matcher, through the grammar's
    // `options.lex.match.string.make`.
    parser.lex_match_factory_ref("@make-toml-string-matcher", make_toml_string_matcher);

    // The grammar declares `val: '@isodate-val'` on its regexp date
    // matchers, so the references have to resolve when the document is
    // installed. The context-aware matchers in `datematcher` are what run,
    // exactly as in the other two ports, and these stay the single
    // description of what a match MEANS.
    parser.value_transform_ref("@isodate-val", isodate_val);
    parser.value_transform_ref("@localtime-val", localtime_val);
}

fn register_state_actions(parser: &mut Tabnas) {
    // The document root.
    parser.state_action_ref("@toml-bo", |rule, _context| {
        rule.node = Rc::new(RefCell::new(new_map()));
        set_path(rule, Path::ROOT);
        Ok(())
    });

    // Allocate this map's node up front. The JSON core allocates a braced
    // map's node in its `@object$` alternate action, but TOML prepends its
    // own map-open alternates that match first and carry no `@object$`, so
    // without this a pushed `pair` would inherit no node at all.
    //
    // SUFFIXED because the engine resolves a bare `@map-bo` to its OWN
    // builtin first and a registration under that name is shadowed
    // silently.
    parser.state_action_ref("@map-bo/append", |rule, _context| {
        rule.node = Rc::new(RefCell::new(new_map()));
        set_path(rule, Path::ROOT);
        Ok(())
    });

    // A table's node is its parent's, which for every table is the
    // document root; the alternate actions then move it to the table being
    // defined. Assigning the path is the whole of it here: the cell is
    // already the parent's, because a pushed or replaced rule shares it.
    parser.state_action_ref("@table-bo", |rule, _context| {
        if let Some(parent) = rule.parent_rule.clone() {
            rule.node = Rc::clone(&parent.node);
        }
        let path = parent_path(rule);
        set_path(rule, path);
        Ok(())
    });

    // A dive has no lifecycle action in the canonical grammar, because
    // there the engine's own node inheritance says what its node is: the
    // parent's when the rule was pushed, the previous rule's when it was
    // replaced. A path has to be inherited explicitly to say the same
    // thing, and one replacement told apart from the other: a dotted key
    // is a replace loop (tabnas/toml#78), so a dive replaced from a
    // segment ending in a dot continues down that segment's table, and a
    // dive replaced through the close loop, from a dive that ended a key,
    // begins the next key from the parent's table again, as the
    // canonical `@dive-key-dot` reads `r.prev.node` or `r.parent.node`.
    parser.state_action_ref("@dive-bo", |rule, _context| {
        let path = if continues_key(rule.prev_rule.as_ref()) {
            prev_path(rule)
        } else {
            parent_path(rule)
        };
        set_path(rule, path);
        Ok(())
    });

    // Fold the table's body into the table, and reset the header counters
    // for whatever comes next.
    parser.state_action_ref("@table-bc", |rule, context| {
        if !truthy(rule.u.get("top_dive")) {
            if let Some(child) = rule.child_rule.clone() {
                let source = child.node.borrow().clone();
                merge_into(context, &cell_of(rule), path_of(rule), &source);
            }
        }

        // The canonical `@table-ac` writes `next.n.table_dive = 0` on the
        // rule the parser moves to. Here `next` arrives as an immutable
        // snapshot, so the reset is made on THIS rule instead, in the
        // before-close phase: the engine builds the replacement rule's
        // counters by sharing this rule's, and it does so after the
        // before-close actions have run. Same rules reset, same values,
        // one phase earlier. No table close alternate reads either
        // counter, so nothing between the two phases can see the
        // difference.
        let counters = rule.n_mut();
        counters.insert("table_dive".to_string(), 0);
        counters.insert("table_array".to_string(), 0);
        Ok(())
    });

    parser.state_action_ref("@dive-bc", |rule, context| {
        if !truthy(rule.u.get("dive_end")) {
            return Ok(());
        }
        let Some(child) = rule.child_rule.clone() else {
            return Ok(());
        };
        let value = child.node.borrow().clone();
        let key = key_of(rule, context);
        write_at(context, &cell_of(rule), path_of(rule), key, value);
        Ok(())
    });
}

fn register_alt_actions(parser: &mut Tabnas) {
    // The five header actions move one `Cursor` along the header, segment
    // by segment, and `@dive-key-dot` below moves one along a dotted key:
    // each resumes the cursor the previous segment parked, and
    // `conclude`s by parking it again, or by writing out what the header
    // created once the header is complete. See `node::Cursor`.
    parser.action_with_context("@table-dive-start", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let mut cursor = Cursor::resume(context, &cell, parent_path(rule));
        let outcome = if table_array(rule) && cursor.has_list(&key) {
            cursor.enter(context, &key);
            cursor.last_of_list(context);
            Ok(())
        } else {
            // A plain-table dive into an existing array of tables walks
            // THROUGH it, which is how `[[x]]` followed by `[x.y]` works.
            cursor.table_at(context, &key, DESCEND)
        };
        conclude(rule, context, &cell, cursor, outcome, false)
    });

    parser.action_with_context("@table-dive-mid", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let mut cursor = Cursor::resume(context, &cell, prev_path(rule));
        if cursor.is_list() {
            cursor.last_of_list(context);
        }
        let outcome = cursor.table_at(context, &key, DESCEND);
        conclude(rule, context, &cell, cursor, outcome, false)
    });

    // A table's body follows the closing bracket at once; an array of
    // tables has its new element still to push, in `@table-cs-push`.
    parser.action_with_context("@table-key-cs-head", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let array = table_array(rule);
        let mut cursor = Cursor::resume(context, &cell, parent_path(rule));
        let outcome = if array {
            cursor.array_at(context, &key)
        } else {
            cursor.table_at(context, &key, DEFINE)
        };
        conclude(rule, context, &cell, cursor, outcome, !array)
    });

    parser.action_with_context("@table-key-cs-tail", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let array = table_array(rule);
        let mut cursor = Cursor::resume(context, &cell, prev_path(rule));
        // The segment before this one may have descended through an array
        // of tables; the key is then defined in its LAST table, as a table
        // for `[a.b.c]` and as an array of tables for `[[a.b.c]]`.
        if cursor.is_list() {
            cursor.last_of_list(context);
        }
        let outcome = if array {
            cursor.array_at(context, &key)
        } else {
            cursor.table_at(context, &key, DEFINE)
        };
        conclude(rule, context, &cell, cursor, outcome, !array)
    });

    parser.action_with_context("@table-cs-push", |rule, context| {
        let cell = cell_of(rule);
        let mut cursor = Cursor::resume(context, &cell, prev_path(rule));
        if !cursor.is_list() {
            // `[[a]]` where `a` is already a scalar. The array itself is
            // produced by `@table-key-cs-head`, so reaching a non-array
            // here means the key conflicts.
            let key = key_of(rule, context);
            let existing = cursor.value();
            cursor.finish(context, &cell);
            return Err(ActionError::new(
                "toml_key_conflict",
                format!(
                    "cannot define {key}, it already has the value {}",
                    existing.to_json()
                ),
            ));
        }
        cursor.push_table(context);
        set_path(rule, cursor.path());
        cursor.finish(context, &cell);
        Ok(())
    });

    parser.action_with_context("@pair-key-set", |rule, context| {
        let key = rule.resolve_open_value(0, context);
        rule.u_mut().insert("key".to_string(), key);
        Ok(())
    });

    // A dotted key moves one cursor along the key as a header does, one
    // segment at a time: each segment resumes the cursor the one before it
    // parked at this rule's path (the parent's for the first segment, the
    // previous segment's table after that, see `@dive-bo`), descends one
    // table and parks it again. `@dive-bc` writes the key's value through
    // `write_at`, which settles the cursor first. Each segment used to walk
    // the tree from the cell's root, so a key cost time growing with the
    // square of its length.
    parser.action_with_context("@dive-key-dot", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let mut cursor = Cursor::resume(context, &cell, path_of(rule));
        let outcome = cursor.table_at(context, &key, DESCEND);
        conclude(rule, context, &cell, cursor, outcome, false)
    });
}

fn register_conditions(parser: &mut Tabnas) {
    parser.alt_condition("@table-top-dive-cond", |rule, _context| {
        let previous = rule
            .prev_rule
            .as_ref()
            .map_or("", |previous| previous.name.as_ref());
        1 == rule.d && "table" != previous
    });
    parser.alt_condition("@lte-table-dive", |rule, _context| {
        rule.lte("table_dive", 0)
    });
    parser.alt_condition("@lte-table-array-1", |rule, _context| {
        rule.lte("table_array", 1)
    });
    parser.alt_condition("@lte-pk", |rule, _context| rule.lte("pk", 0));
    parser.alt_condition("@map-is-table-parent", |rule, _context| {
        rule.parent_rule
            .as_ref()
            .is_some_and(|parent| "table" == parent.name.as_ref())
    });
}

fn register_routes(parser: &mut Tabnas) {
    // `!r.n.table_array && 'map'` and `r.n.table_array && 'table'`: a
    // falsy answer means "no route", which is `None` here.
    parser.alt_push("@table-end-p", |rule, _context| {
        (0 == rule.n.get("table_array").copied().unwrap_or(0)).then(|| "map".to_string())
    });
    parser.alt_replace("@table-end-r", |rule, _context| {
        (0 < rule.n.get("table_array").copied().unwrap_or(0)).then(|| "table".to_string())
    });
}
