# Agents Guide: rs/

The Rust port of the canonical TypeScript in [`../ts`](../ts). Read
[`../AGENTS.md`](../AGENTS.md) first: it holds the cross-runtime rules,
the grammar-embed rule and the conformance-suite rules, and this file
only covers what is specific to this crate.

## Layout

| Path | |
|---|---|
| `src/lib.rs` | the embedded grammar, the document adjustments, the depth guard (`DEPTH_LIMIT`, `DEPTH_GUARD`) and the unit tests of its kept count, `toml`, `plugin`, `make`, `make_with`, `parse`, `VERSION`, and the translation parts `manifest_text` and `render_text`, `include_str!` of the copies in `translate/` |
| `src/refs.rs` | every `@`-named reference the grammar uses: state actions, alternate actions, conditions, conditional `p:`/`r:` targets |
| `src/node.rs` | table nodes as PATHS (see below), the cursor that keeps a dotted header and a dotted key linear, and the key-conflict diagnosis |
| `src/strmatcher.rs` | TOML's basic, literal and multi-line strings |
| `src/datematcher.rs` | the context-aware date and time matchers, and the leading-BOM matcher |
| `src/daterange.rs` | whether a date or time whose SHAPE matched denotes a real instant |
| `src/values.rs` | `TomlTime`, and how it rides on a `tabnas::Value` |
| `translate/` | the crate's copies of `../tabnas.plugin.json` (as `manifest.json`) and `../alchemy/render.alc`, which a packaged crate needs; `tests/translate_test.rs` holds them to the files |
| `tests/parity_test.rs` | every `../test/spec/*.tsv` fixture, discovered by listing |
| `tests/toml_test.rs` | in-language behaviour: API, special floats, triple quotes, date kinds, BOM, error columns and rows, key conflicts, the depth guard, the canonical message templates, the embedded grammar, threads |
| `tests/toml_valid_test.rs` | the BurntSushi/toml-test corpus, both halves |
| `tests/divergent_test.rs` | the divergence register, `rust` column |
| `tests/translate_test.rs` | the translation parts: the render the embedded manifest names is the one `render_text()` embeds, the manifest's shapes and loss lines, and every render definition named `toml-...` |
| `tests/perf_test.rs` | `parse` reuses its instance; a parse, a dotted header and a dotted key take time in proportion to their length; a header and a key 5,000 segments deep; a key keeps rule depth constant over 10,000 segments |
| `tests/version_test.rs` | Cargo.toml == `VERSION` == ts/package.json |
| `tests/common/mod.rs` | shared helpers: spec dir, repo dir, value and failure conversion |
| `README.md` | the crate front page, prose-gated; its `rust` fences are doctests of this crate |

Crate `tabnas-toml`, library `tabnas_toml`. The engine (`tabnas`), the
jsonic core (`tabnas-jsonic`) and the fixture runner (`tabnas-support`,
dev only) are **path dependencies on sibling checkouts**
(`../../parser/rs`, `../../jsonic/rs`, `../../support/rs`). None is
published, so there is no registry version to fall back on.

A FOURTH checkout, `../../json/rs`, is needed and is named by no entry
here: `tabnas-jsonic` takes the strict-JSON core as its own path
dependency. Cargo reads the whole manifest graph before compiling, so
without it every cargo command fails at `failed to get tabnas-json as a
dependency of package tabnas-jsonic`. `ci/rust/run.sh` checks all four,
and `the_setup_instructions_name_every_sibling_checkout` in
`tests/toml_test.rs` holds both READMEs to the set the manifests imply.

```bash
cargo build --all-targets
cargo test --all-targets && cargo test --doc
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

`make test-rs` from the repository root is the fast loop; `ci/rust/run.sh`
is the full gate and adds `fmt --check`, the lockfile check and the MSRV
pin.

## THE BIG ONE: a table node is a PATH, not a reference

Read this before touching `refs.rs` or `node.rs`.

The canonical grammar hands a table rule a REFERENCE to a nested object
(`r.node = r.parent.node[key]`) and then lets that table's pairs be
written straight into it. JavaScript objects alias; so does Go's
`*OrderedMap`. A `tabnas::Value` does not: its containers are
`Arc<IndexMap>` and `Arc<Vec>`, mutated through `Arc::make_mut`, which
COPIES as soon as a second handle exists. A clone of a nested map is a
snapshot, and writes to it never reach the document.

So here a node is a CELL plus a PATH: the `Rc<RefCell<Value>>` the rule
already carries, and the keys and indices from that cell's value down to
the table. A write walks the path from the cell, so it lands in the real
tree however the `Arc`s happen to be shared. A rule keeps its path in its
`u` bag under `node::PATH_KEY`, because `u` is per-rule and merged key by
key, so an alternate's own `u: { ... }` does not disturb it. What it keeps
there is a `node::Path`: two numbers naming a prefix of a buffer in the
parse's path registry, which lives in the context's `u` bag.

The translation is mechanical, and each canonical line has exactly one
form here:

| canonical | here |
|---|---|
| `r.node = r.parent.node` | `set_path(rule, parent_path(rule))` |
| `r.node = tableAt(r.parent.node, key, …)` | `cursor.table_at(context, &key, …)?` on a `Cursor` at `parent_path(rule)` |
| `Array.isArray(r.prev.node)` | `cursor.is_list()` on a `Cursor` at `prev_path(rule)` |
| `Object.assign(r.node, r.child.node)` | `merge_into(context, &cell, path_of(rule), &child_value)` |
| `r.node[key] = r.child.node` | `write_at(context, &cell, path_of(rule), key, child_value)` |
| `r.prev.node.push(node())` | `cursor.push_table(context)` on a `Cursor` at `prev_path(rule)` |

Three rules follow from it:

- **A write creates only its FINAL segment.** Every caller builds the
  parent first, through `table_at` or `array_at`. A write whose parent
  does not exist is a silent no-op, which is what the first cut of this
  got wrong: `[a]` produced `{}` because the write navigated to a key that
  was not there yet. A `Cursor` reproduces those no-ops exactly: a
  position whose parent cannot take it is `Absent`.
- **Never hold a cloned sub-value across a write.** Reads clone, and a
  clone bumps the `Arc`, so a write while one is alive copies a container
  it did not need to. A `Cursor` holds one between two segments of a
  header, deliberately, and lets go of it before anything writes.
- **A header costs time in proportion to its length.** See the next
  section.

## A dotted header is linear, and what that costs in exactness

Each segment of a header used to call `prev_path`, which rebuilt the whole
path from the rule's `u` bag, walk the tree from the cell's root once or
twice to look at and create the next table, and store a fresh copy of the
longer path on its rule. The engine keeps every rule a replace loop passes
through (`prev_rule`, unbounded by default), so a header of n segments
also held n paths of up to n segments. In a release build 2,000 segments
took a second and 10,000 two minutes and several gigabytes, where the other
two ports, handed `r.prev.node` by reference, take milliseconds
(tabnas/toml#81). Two things in `node.rs` make it O(n):

- **Paths are handles, not copies.** A path buffer only ever grows, so
  every prefix stays valid, and extending the newest path in a buffer
  appends in place. Storing a path on a rule costs two numbers.
- **The five header actions move one `Cursor`.** Each resumes the cursor
  the previous segment parked in the context's `u` bag and `conclude`s by
  parking it again. While a header walks through tables that exist, the
  cursor holds a read-only handle on the node it has reached: nothing
  writes to the tree between two segments of a header. Once a segment
  creates a table, every later one lands in something just created, which
  is empty, so nothing is looked up and nothing can conflict. The created
  containers are recorded and written in ONE walk when the header ends,
  when an action fails (so a diagnostic is raised over the same tree as
  before), or before anything else reads or writes the tree (`settle`).

The document is the same, table for table and key for key, because each
table is created in the same container, under the same key, in the same
order. What moves is the moment the created tables reach the tree: from
each segment to the end of the header. No parse result can see that,
except a parse that RECOVERS from errors, which reads the whole tree after
every action as its partial value. So under `parse.recover.enabled`,
`Cursor::park` writes everything out at the end of each action instead,
and the tree after every action is what it always was. That mode keeps the
old O(path) per segment; it already copies the spine of the tree on every
write, because the partial value it holds shares it.

The proof is in `tests/perf_test.rs`:
`a_dotted_header_takes_time_in_proportion_to_its_length` compares 2,000
segments with 8,000 against a limit of 8x (linear is about 4x; the old
code fails it within a minute and a half, in a debug build as in a release
one, because the long header is parsed against a deadline), and
`a_long_dotted_header_builds_every_table_it_names` pins the value and the
diagnosis of headers 5,000 segments deep. Both mirror tests in the other
two ports, and both lift the depth guard (next section) first: they
measure the algorithm, which has to be linear whatever the limit is.

## A dotted key is the same loop, and parks on a stack

A dotted key (`a.b.c = 1`, the `dive` rule) used to be a push chain, a
`dive` pushed per segment, so rule depth grew with the key, 10,002 for
ten thousand segments, and each segment walked the tree from the cell's
root, so a key cost time growing with the square of its length
(tabnas/toml#78). The grammar now re-enters `dive` in the same frame per
segment, as `table` does for a header, and `@dive-key-dot` moves the same
`Cursor` the header actions move: it resumes the cursor the previous
segment parked at this rule's path, descends one table and parks it again,
and `@dive-bc` writes the key's value through `write_at`, which settles
the cursor first.

Two things differ from the header. `@dive-bo` has to tell the two ways a
dive is replaced apart: from a segment ending in a dot it inherits the
previous segment's path (`continues_key`), and from the close loop, which
takes the next dotted key without returning to the pair, it starts from
the parent's path again, as the canonical `@dive-key-dot` reads
`r.prev.node` or `r.parent.node`. And the parked cursors are a STACK in
the context's `u` bag (`PARKED_KEY`), not a slot: a key's value can be an
inline table whose own dotted keys park a cursor each before the outer key
is done (`x.y = {p.q = {m.n = 1}}`), and with one slot the inner key
clobbered the outer, whose tables were then never written. `settle` and
`Cursor::resume` take the cursor parked last and put it back when it is
parked on another cell. `a_dotted_key_keeps_rule_depth_constant`,
`a_dotted_key_takes_time_in_proportion_to_its_length` and
`a_long_dotted_key_builds_every_table_it_names` are the proof, mirrored in
both other ports, and the fixture rows in `../test/spec/dotted-keys.tsv`
walk the loop and the nested inline tables in every runtime.

## Nesting is bounded at 127 levels

`toml()` installs a parse guard under the name `depth`, the name jsonic
installs its own under, so this one replaces it, as YAML's does. jsonic's
counts its `map` and `list` rules, which is every inline table, every array
and every table's body, but a header and a dotted key nest through `table`
and `dive`, which it never saw, so a key of 10,000 segments parsed.
`DEPTH_LIMIT` levels parse and the next one is refused with the engine's
`cancel`, as every grammar in the fleet refuses depth in Rust (aless reads
that `cancel` as `too_deep`, and the transducer as `INPUT_INVALID` naming
the grammar's guard). TypeScript and Go have no limit, and the two rows of
`../test/divergent.tsv` record it as permanent.

`depth` (in `lib.rs`) counts the root table and what each rule adds: every
rule on the stack (`frame_levels`), and the rule the loop is working on
(`current_levels`), which the engine hands over apart from the stack. A
`map` or a `list` adds one, except a `map` directly above a `table`, which
is that table's body and sits INSIDE the table at the end of its path. A
`table` or a `dive` adds the length of its path, kept in `u`.

**A dive is counted by its path, never by a counter.** A dive's path
counts the tables its key has descended through from the table the key is
in, so every dotted key still open counts, each from where the one outside
it left off. The first cut of this guard read a `dive_key` counter off the
current rule instead, which every rule under a dive inherited, and the
dive's close loop, which takes the next dotted key in the same frame and
has to begin it from the table it is in, reset that counter to zero. Inside
an inline table that is the value of another key, the reset dropped the
OUTER key's tables too: `k0.….k99 = {p.q = 1`, a newline and
`b0.….b99 = 1}`, which this grammar accepts, parsed at 200 levels, and 26
levels of 99-segment keys built a value 2,576 deep whose display ended the
process with a stack overflow. A key after a comma begins with a new
`pair`, which inherited the count, so only the newline and space forms got
through. The counter is gone from the grammar, since nothing reads it, and
`a_key_the_close_loop_takes_counts_from_the_keys_outside` in
`tests/toml_test.rs` pins those shapes.

The current rule is the one place a path says nothing yet: the engine runs
the guard before any of the step's actions, and a replacement inherits
counters but not `u`. A `table` that is the current rule is still reading
its header, one segment per replacement, so the `table_dive` counter, plus
one for the segment being read, says how deep the header has got. A `dive`
that is the current rule and still open has not run `@dive-bo`: it stands
where the segment it replaced left off when it continues a key
(`continues_key`, the question `@dive-bo` asks), and at nothing when it
begins one. That is what refuses a header or a key AT the segment past the
limit, before any value past it is built; read the paths there and a
10,000-segment header is parsed to its end, written into the tree 10,000
deep, and only then refused, with the deep value left to drop.

The count over the stack is KEPT from one step to the next
(`levels_on_stack`), as jsonic keeps its own (tabnas/jsonic#91): a frame's
levels depend on it and the frame below it alone, the engine changes the
stack only at the top, and a step recounts from the first position whose
rule id changed. Recounting the whole stack at every step, as the first cut
did, made 2,000 lines of arrays or inline tables nested 120 deep take 20
to 28 percent longer than under jsonic's guard in a release build; kept,
they take what they took there. Two unit tests in `lib.rs` hold it:
`the_kept_count_is_the_walked_count_at_every_step` compares it with a walk
at every step, and `a_step_counts_what_changed_not_the_whole_stack` counts
the frames a step looks at.

The boundaries, in container terms: a dotted key of 127 segments parses
and 128 is refused (the root table is a level, so n segments nest n
levels); a header of 126 parses and 127 is refused (its tables sit in the
root); an array of tables is one level more; an inline table or an array
in the root table stops at 126, where jsonic's guard stopped it.
`nesting_is_bounded_by_the_depth_guard` in `tests/toml_test.rs` pins them,
with the mixed cases, width against depth, and the shared default parser.

**Lifting the guard lifts every bound.** A caller that wants the depth
calls `remove_parse_guard(DEPTH_GUARD)`, and because this guard replaced
jsonic's under the same name, nothing is left: not the bound on headers
and dotted keys this guard adds, and not the one jsonic's guard kept on
inline tables and arrays either. 127 nested arrays, which jsonic's guard
refused, parse, and so do 10,000. The caller drops the value on a stack
that can take it, which is what the perf tests do.

## Two lifecycle actions that are not where the canonical ones are

**`@dive-bo` has no canonical counterpart.** There, the engine's own node
inheritance says what a pushed or replaced `dive` rule's node is: the
parent's when it was pushed, the previous rule's when it was replaced. A
path has to be inherited explicitly to say the same thing, and the engine
sets `prev_rule` only on a replace. A dive is replaced in two ways, and
`continues_key` tells them apart: from a segment ending in a dot, which
continues down the previous segment's table, and from the close loop,
from a dive that ended a key (`dive_end`), which begins the next key from
the parent's table again, as the canonical `@dive-key-dot` reads
`r.prev.node` or `r.parent.node`.

**`@table-ac` is done in `@table-bc`.** The canonical handler writes
`next.n.table_dive = 0` on the rule the parser moves to next. Here `next`
arrives as an immutable `&RuleSnapshot`, and even a mutable one would not
help: the engine builds the replacement rule's counters with
`Rc::clone(&current.n)`, so a `make_mut` in the after phase would split
the two rather than reach the next rule. The reset is therefore made on
THIS rule in the BEFORE-close phase, which the engine runs before it
clones the counters, so the next rule gets them. No table close alternate
reads either counter, so nothing between the two phases can see the
difference. The test that this still works is the `array-of-tables.tsv`
fixture: without it, a second `[[a]]` never starts a new element.

## What the grammar document needs before it is installed

`grammar_document` parses `../toml-grammar.jsonic` with a standard jsonic
instance, exactly as the other two ports do, and then makes five
adjustments. Each is in `lib.rs` next to its reason; the short version:

1. **`integerize`.** Every number in a jsonic parse result is an `f64`,
   so `b: 2` arrives as `2.0` and the grammar loader refuses it: a
   backtrack count must be a non-negative integer. Go does the same thing
   one field at a time in `mapToAlt`.
2. **`adjust_token_sets`.** The canonical token-set form pads a set to
   four slots with nulls; this loader takes the members alone and rejects
   a null entry.
3. **`adjust_value_matchers`.** The date and time patterns in
   `match.value` are respelled with `[0-9]`. See "ASCII digits" below.
4. **`adjust_lex_matchers`.** The grammar's `lex.match.string.make` has
   no `order`, because in the canonical engine the string matcher is a
   named builtin being REPLACED. Here the builtin bands are fixed and a
   custom matcher is placed among them by order, so one is supplied. The
   BOM and date matchers are added here rather than to the shared grammar
   text, because that text is read by two ports that install their own.
5. **`adjust_messages`.** The `error` and `hint` templates, kept in step
   with the TypeScript registration word for word, which
   `the_error_templates_are_the_canonical_ones` in `tests/toml_test.rs`
   measures: it reads the installed options off a live instance and looks
   for each template in `../ts/src/toml.ts`.

`register_special_floats` runs AFTER the document, because a keyword
value definition takes a literal `val` and never a function reference, so
there is no `@`-name for a number JSON cannot spell. Both other ports
patch `nan` and `inf` in code for the same reason.

## ASCII digits: never `\d` in a pattern this crate compiles

The `regex` crate is Unicode-aware by default, so `\d` is the whole `Nd`
category, `\w` is `Alphabetic|M|Nd|Pc`, `\s` is `White_Space` and `\b`
sits on those. The JavaScript this port comes from compiles its patterns
WITHOUT the `u` flag and the Go port is RE2, so in both of those every
one of those classes is ASCII. Porting such a pattern verbatim silently
widens it.

It is not a theoretical widening. `٢٠٢٤-٠١-٠١ = 1` (Arabic-Indic digits)
was ACCEPTED as a bare key here, because in a key context the date
matcher emits what it matched as `#ID` directly, bypassing the `#ID`
token pattern that is `[a-zA-Z0-9_-]+`; both other runtimes answer
`unexpected`. In a value context the same input was rejected as
`invalid_datetime`, because `daterange` captured the components and then
could not parse them, so the month read as -1.

So every pattern this crate compiles spells its digits `[0-9]`: the two
shapes in `datematcher`, the two capture shapes in `daterange`, and the
two `match.value` patterns the grammar text declares, which
`adjust_value_matchers` respells from the `datematcher` patterns
themselves so the two cannot drift. The shared grammar text keeps `\d`,
which is correct for the runtimes that read it.

`only_ascii_digits_make_a_date_or_time` in `tests/toml_test.rs` pins the
part no fixture can: a `TomlTime` and a string flatten to the same JSON,
so only the KIND separates a local time from text that looks like one.
The rest is in `../test/spec/errors.tsv` and
`../test/spec/basic-values.tsv`.

Nothing else in the crate has this hazard: `strmatcher` uses
`is_ascii_alphanumeric`, `is_ascii_hexdigit` and `char::to_digit`, all
three ASCII-only and all three matching what the canonical `isHexadecimal`
and `parseInt` do.

## What this port does NOT need

- **No `injectIDLexGuards`.** The Go port prepends a never-matching
  alternate with `#ID` at slot 0, because its `matchMatch` only consults
  slot 0 when deciding whether a custom-regex token is expected. This
  engine's `expected_match_tins` is per-slot, as TypeScript's is, so `b`
  in `[b]` lexes as `#ID` with no help.
- **No `stripUnsupported` for the string matcher.** The Go port removes
  `options.lex.match.string` because it has no way to resolve the
  reference; here `lex_match_factory_ref` resolves it, so the grammar's
  own declaration is what installs the matcher.
- **No comment-definition repair.** This loader treats
  `comment.def.slash: null` as a removal, as TypeScript does, so `#`
  survives. The Go loader treats a non-nil `comment.def` as a full
  replacement and has to re-add `hash`.

## A string token's point is its END

`strmatcher` builds its `#ST` token from the cursor AFTER the string, not
before it. That reads like a bug and is the canonical behaviour: the
TypeScript matcher writes its finished `sI`/`rI`/`cI` back into `pnt` and
only then builds the token, and the Go matcher does the same. So a
diagnostic about the token after a quoted key points at the end of the
key, which is what `go/strmatcher_col_test.go` and the TypeScript 'error
columns count characters, not bytes' test both measure. Building the
token from the point captured on entry moves every one of those columns
back to the opening quote, and `string_error_columns_count_scalars_not_bytes`
in `toml_test.rs` is what says so.

The scan itself hands the engine a COUNT of scalars and lets
`advance_chars` do the row and column arithmetic. The Go port walks bytes
and owns that arithmetic itself, which is why it needs a test for it;
here there is no second implementation of it to drift.

## The lax hex scan in `\u` is deliberate

`is_hexadecimal_lax` accepts every ASCII letter, not only `a`-`f`,
because the canonical `isHexadecimal` does. The range check below it is
what rejects `\uZZZZ`, and the digits a lax scan lets through are then
read by `hex_prefix`, which stops at the first non-hex character exactly
as `parseInt` does. Narrowing it to real hex digits would change which
documents parse: `"\u12g4"` is U+0012 in TypeScript and here, and
`invalid_unicode` in Go. That is a TypeScript/Go disagreement this port
did not create and does not adjudicate; it reproduces TypeScript, which
is the rule. It is a row of `../test/divergent.tsv` now, measured in all
three ports.

**Go's behaviour is the repair target, not the defect.** TOML requires
exactly four hexadecimal digits after `\u`, so `"\u12g4"` is not a TOML
document, and Go is the only one of the three that says so. Under ADR-13
this is the case where the TypeScript implementation is itself defective
and TypeScript moves; this port follows it there. Reading the row as
"repair Go" would send somebody to change the one conforming port.

## The divergence register has a local runner

`tests/divergent_test.rs` carries its own comparator rather than using
`tabnas_support::Register`, and so do the other two halves. That library
compares two cells by error CODE and drops the `@row:col` suffix; the
astral rows of this register are positional disagreements on one code, so
it reads them as "records no divergence" and they assert nothing.
`same_expectation_reads_the_position` pins the comparator, because a
comparator that stops distinguishing positions does not fail, it just
makes the register vacuous. When `tabnas_support` compares positions,
delete the local comparator in all three halves together.

## The remaining lookahead divergence is positional

The engine now remembers a later `TIN_BD` token while matching alternates
and preserves its diagnosis when none match. That closed two former rows
of `../test/divergent.tsv` and the code difference in the third.

One position difference remains. For
`a = '''x''''''''''''''`, leftovers from the first literal string re-lex
into a second `#ST`. TypeScript and Go report
`unterminated_string@1:18`, while this port reports the same code at
`1:22`, the re-lexed token's end. The register pins that position until
the engines agree about which point represents this failure.

## The conformance suite never skips

`tests/toml_valid_test.rs` runs `../scripts/fetch-toml-test.sh` itself
when the corpus is missing and FAILS if it still is. Do not reintroduce a
skip: both older suites used to skip when the corpus was absent, which is
exactly what CI looked like, so neither had ever executed there while the
job reported green.

The fetch runs AT MOST ONCE, behind a `OnceLock` in `ensure_corpus`. The
script is not concurrency-safe: it removes a destination that is not a
git checkout and then clones into it, so on a fresh checkout `toml_valid`
and `toml_invalid` each saw the corpus absent, each launched it, and the
loser's `git clone` exited 128 into the directory the winner had just
made. A process-local guard is enough BECAUSE both callers are in this
one integration-test binary, which the default harness runs as two
threads of one process; add a caller in another `tests/*.rs`, or run the
suite under a test-per-process runner, and only a lock on the
destination serialises them.
`the_corpus_is_fetched_once_however_many_tests_ask` is what measures it.

The `rust` row of `../test/conformance.tsv` is EXACT, not a floor, for
the reason that file's header gives. It reproduces the `ts` row.

## The docs are gated

`README.md` is in the published set: no em dashes in prose, no first
person singular, no links to any `AGENTS.md`, no project history. This
file is internal and may be blunt.

## The README is doctested

`src/lib.rs` includes `README.md` as rustdoc under `#[cfg(doctest)]`, so
every `rust` fence in it runs on `cargo test --doc` (they show up as
`readme_examples (line N)`). rustdoc runs each fence as written, so a
fence must be a complete program: wrap it in
`fn main() -> Result<(), Box<dyn std::error::Error>> { ... Ok(()) }`
rather than using `?` at the top level, and never use hidden `# ` lines,
which render as garbage on GitHub. The `toml` and `bash` fences are not
run.
