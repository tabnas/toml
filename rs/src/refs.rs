// Copyright (c) 2021-2026 Richard Rodger, MIT License

//! The function references the declarative grammar names with `@`.
//!
//! Registered on the instance BEFORE the grammar document is installed,
//! because the document is what looks them up. The state actions are wired
//! by the `@<rule>-<phase>` convention; the alternate actions, conditions
//! and conditional `p:` / `r:` targets are named explicitly.

use std::cell::RefCell;
use std::rc::Rc;

use tabnas::{ActionError, Context, Rule, Tabnas, Value};

use crate::node::{
    self, array_at, last_of_list, merge_into, new_map, path_of, push, read, set_path,
    snapshot_path, table_at, Seg, DEFINE, DESCEND,
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
fn cell_of(rule: &Rule) -> Rc<RefCell<Value>> {
    Rc::clone(&rule.node)
}

/// The path this rule's PARENT sits at.
fn parent_path(rule: &Rule) -> Vec<Seg> {
    snapshot_path(rule.parent_rule.as_ref())
}

/// The path the rule this one REPLACED sat at.
fn prev_path(rule: &Rule) -> Vec<Seg> {
    snapshot_path(rule.prev_rule.as_ref())
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
        set_path(rule, &[]);
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
        set_path(rule, &[]);
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
        set_path(rule, &path);
        Ok(())
    });

    // A dive has no lifecycle action in the canonical grammar, because
    // there the engine's own node inheritance says what its node is: the
    // parent's when the rule was pushed, the previous rule's when it was
    // replaced. A path has to be inherited explicitly to say the same
    // thing.
    parser.state_action_ref("@dive-bo", |rule, _context| {
        let path = if rule.prev_rule.is_some() {
            prev_path(rule)
        } else {
            parent_path(rule)
        };
        set_path(rule, &path);
        Ok(())
    });

    // Fold the table's body into the table, and reset the header counters
    // for whatever comes next.
    parser.state_action_ref("@table-bc", |rule, _context| {
        if !truthy(rule.u.get("top_dive")) {
            if let Some(child) = rule.child_rule.clone() {
                let source = child.node.borrow().clone();
                let path = path_of(rule);
                merge_into(&cell_of(rule), &path, &source);
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
        let mut path = path_of(rule);
        path.push(Seg::Key(key));
        node::write(&cell_of(rule), &path, value);
        Ok(())
    });
}

fn register_alt_actions(parser: &mut Tabnas) {
    parser.action_with_context("@table-dive-start", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let parent = parent_path(rule);

        let mut candidate = parent.clone();
        candidate.push(Seg::Key(key.clone()));

        let path = if 0 < rule.n.get("table_array").copied().unwrap_or(0)
            && node::is_list(&cell, &candidate)
        {
            last_of_list(&cell, &candidate)
        } else {
            // A plain-table dive into an existing array of tables walks
            // THROUGH it, which is how `[[x]]` followed by `[x.y]` works.
            table_at(&cell, &parent, &key, DESCEND)?
        };
        set_path(rule, &path);
        Ok(())
    });

    parser.action_with_context("@table-dive-mid", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let previous = prev_path(rule);

        let base = if node::is_list(&cell, &previous) {
            last_of_list(&cell, &previous)
        } else {
            previous
        };
        let path = table_at(&cell, &base, &key, DESCEND)?;
        set_path(rule, &path);
        Ok(())
    });

    parser.action_with_context("@table-key-cs-head", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let parent = parent_path(rule);
        let path = if 0 < rule.n.get("table_array").copied().unwrap_or(0) {
            array_at(&cell, &parent, &key)?
        } else {
            table_at(&cell, &parent, &key, DEFINE)?
        };
        set_path(rule, &path);
        Ok(())
    });

    parser.action_with_context("@table-key-cs-tail", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let previous = prev_path(rule);

        let path = if node::is_list(&cell, &previous) {
            let last = last_of_list(&cell, &previous);
            table_at(&cell, &last, &key, DEFINE)?
        } else if 0 < rule.n.get("table_array").copied().unwrap_or(0) {
            array_at(&cell, &previous, &key)?
        } else {
            table_at(&cell, &previous, &key, DEFINE)?
        };
        set_path(rule, &path);
        Ok(())
    });

    parser.action_with_context("@table-cs-push", |rule, context| {
        let cell = cell_of(rule);
        let previous = prev_path(rule);
        if !node::is_list(&cell, &previous) {
            // `[[a]]` where `a` is already a scalar. The array itself is
            // produced by `@table-key-cs-head`, so reaching a non-array
            // here means the key conflicts.
            let key = key_of(rule, context);
            let existing = read(&cell, &previous).unwrap_or(Value::Undefined);
            return Err(ActionError::new(
                "toml_key_conflict",
                format!(
                    "cannot define {key}, it already has the value {}",
                    existing.to_json()
                ),
            ));
        }
        let Some(index) = push(&cell, &previous, new_map()) else {
            return Ok(());
        };
        let mut path = previous;
        path.push(Seg::Index(index));
        set_path(rule, &path);
        Ok(())
    });

    parser.action_with_context("@pair-key-set", |rule, context| {
        let key = rule.resolve_open_value(0, context);
        rule.u_mut().insert("key".to_string(), key);
        Ok(())
    });

    parser.action_with_context("@dive-key-dot", |rule, context| {
        let key = key_of(rule, context);
        let cell = cell_of(rule);
        let parent = parent_path(rule);
        let path = table_at(&cell, &parent, &key, DESCEND)?;
        set_path(rule, &path);
        Ok(())
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
    parser.alt_condition("@lte-dive-key-1", |rule, _context| rule.lte("dive_key", 1));
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
