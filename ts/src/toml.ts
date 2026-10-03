/* Copyright (c) 2021-2025 Richard Rodger, MIT License */

// The engine is the tabnas parser; jsonic supplies the relaxed-JSON
// grammar that the embedded grammar text is authored in. Engine types
// (Plugin, Rule, Lex) and the EMPTY constant come from @tabnas/parser.
import { Tabnas, Rule, Lex, Plugin, EMPTY } from '@tabnas/parser'
import { jsonic, JsonicError } from '@tabnas/jsonic'

// See defaults below for commentary.
type TomlOptions = {}

// --- BEGIN EMBEDDED toml-grammar.jsonic ---
const grammarText = `
# TOML Grammar Definition
# Parsed by a standard Jsonic instance and passed to jsonic.grammar()
# Function references (@ prefixed) are resolved against the refs map.
# Regex references (@/pattern/flags) are resolved to RegExp instances.

{
  options: rule: { start: toml exclude: jsonic }
  options: lex: {
    emptyResult: {}
    match: string: make: '@make-toml-string-matcher'
  }
  options: fixed: token: { '#CL': '=' '#DOT': '.' }
  options: match: {
    token: { '#ID': '@/^[a-zA-Z0-9_-]+/' }
    value: {
      isodate: {
        match: '@/^\\\\d\\\\d\\\\d\\\\d-\\\\d\\\\d-\\\\d\\\\d([Tt ]\\\\d\\\\d:\\\\d\\\\d(:\\\\d\\\\d(\\\\.\\\\d+)?)?([Zz]|[-+]\\\\d\\\\d:\\\\d\\\\d)?)?/'
        val: '@isodate-val'
      }
      localtime: {
        match: '@/^\\\\d\\\\d:\\\\d\\\\d(:\\\\d\\\\d(\\\\.\\\\d+)?)?/'
        val: '@localtime-val'
      }
    }
  }
  options: tokenSet: {
    KEY: ['#ST' '#ID' null null]
  }
  options: comment: def: { slash: null multi: null }

  rule: toml: open: [
    { s: ['#ST #NR #ID' '#CL'] p: table b: 2 }
    { s: ['#OS' '#ST #NR #ID'] p: table b: 2 }
    { s: ['#OS' '#OS'] p: table b: 2 }
    { s: ['#ST #NR #ID' '#DOT'] p: table b: 2 }
    { s: '#ZZ' }
  ]

  rule: table: {
    open: [
      { s: ['#ST #NR #ID' '#CL'] p: map b: 2 }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 }
      { s: ['#OS' '#OS'] r: table n: { table_array: 1 } }
      {
        s: ['#ST #NR #ID' '#DOT']
        c: '@table-top-dive-cond'
        p: dive
        b: 2
        u: { top_dive: true }
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        c: '@lte-table-dive'
        n: { table_dive: 1 }
        a: '@table-dive-start'
        g: 'dive,start'
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        n: { table_dive: 1 }
        a: '@table-dive-mid'
        g: 'dive'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        c: '@lte-table-dive'
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-head'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-tail'
        g: 'dive,end'
      }
      {
        s: '#CS'
        p: map
        c: '@lte-table-array-1'
        a: '@table-cs-push'
      }
    ]
    close: [
      { s: ['#OS' '#OS'] r: table b: 2 g: end }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 g: end }
      { s: '#ZZ' g: end }
    ]
  }

  rule: map: {
    open: [
      { s: '#OS' b: 1 }
      {
        s: ['#ST #NR #ID' '#CL']
        c: '@map-is-table-parent'
        p: pair
        b: 2
      }
      { s: ['#OB' '#ST #NR #ID'] b: 1 p: pair }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
      { s: '#ZZ' }
    ]
    close: [
      { s: '#OS' b: 1 g: end }
      { s: '#ZZ' g: end }
    ]
  }

  rule: pair: {
    open: [
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        u: { pair: true }
        a: '@pair-key-set'
      }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
    ]
    close: [
      { s: ['#ST #NR #ID'] b: 1 r: pair g: comma }
      { s: ['#CA' '#ST #NR #ID'] b: 1 r: pair g: comma }
      { s: ['#OS'] b: 1 g: end }
      { s: ['#CA' '#CB'] c: '@lte-pk' b: 1 g: close }
    ]
  }

  rule: val: close: [
    { s: ['#ST #NR #ID'] b: 1 g: end }
    { s: ['#OS'] b: 1 g: end }
  ]

  rule: elem: close: [
    { s: ['#CA' '#CS'] b: 1 g: comma }
  ]

  rule: dive: {
    open: [
      {
        s: ['#ST #NR #ID' '#DOT']
        p: dive
        n: { dive_key: 1 }
        a: '@dive-key-dot'
      }
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        n: { dive_key: 1 }
        u: { dive_end: true }
      }
    ]
    close: [
      {
        s: ['#ST #NR #ID' '#DOT']
        b: 2
        r: dive
        c: '@lte-dive-key-1'
        n: { dive_key: 0 }
      }
      {}
    ]
  }
}
`
// --- END EMBEDDED toml-grammar.jsonic ---

// Plugin implementation.
// TOML allocates every map and table here rather than leaning on the core's
// allocator, so it must follow the core's convention too: nodes carry no
// prototype ("no prototype, like JSON" - @tabnas/parser builtins).
//
// With a plain `{}` literal a table named __proto__ is not an ordinary key.
// `node['__proto__']` reads back Object.prototype, which is truthy, so the
// `|| {}` reuse guard below hands that back as the table node and every pair
// in it is written onto Object.prototype - polluting every object in the
// process from a single parsed document. Allocating without a prototype makes
// __proto__ an ordinary own key, which is what jsonic, json5, yaml and zon
// already do.
const node = () => Object.create(null)

// The last table of an array-of-tables, or a DIAGNOSED refusal to append to
// something that is not an array. `[[a]]` after `a = 1` raised
// "r.prev.node.push is not a function" — the same crash class as tableAt's,
// reached through .push instead of an assignment. Raised on the segment's
// own token, `r.o0`, as tableAt's refusals are.
function arrayAt(container: any, key: string, r: any, ctx: any): any[] {
  const existing = container[key]
  if (Array.isArray(existing)) return existing
  if (null == existing) return (container[key] = [])

  throw new JsonicError(
    'toml_key_conflict',
    { key, why: `it already has the value ${JSON.stringify(existing)}` },
    r.o0, r, ctx)
}

// What a segment asks of the table under its key.
//
// A header like `[fruit.variety]` walks THROUGH `fruit` and DEFINES `variety`,
// and a dotted key like `fruit.variety = 1` walks through `fruit` as well.
// The three positions have different rules. Descending through an
// array-of-tables is how `[[x]]` followed by `[x.y]` works, and four valid
// corpus documents rely on it; landing on one as the thing being defined is
// `[x]` trying to redefine `[[x]]` — invalid TOML. A table a header walks
// through and finds missing is created implicitly, and TOML lets one later
// header define it (`[a.b]` then `[a]`); a table a header has DEFINED, a
// dotted key has created or walked through, or an inline table has made may
// not be defined by a header again (`[a]` twice, `a.b = 1` then `[a]`,
// `a = {}` then `[a]`).
//
// The grammar already separates the positions: `#DOT`-terminated segments of
// a header are intermediate, `#CS`-terminated ones are final, and a dotted
// key's segments are the `dive` rule's.
const DESCEND = 'descend'
const DEFINE = 'define'
const DIVE = 'dive'

// The tables a header's prefix created and no header has yet defined: the
// only existing tables a header may define. Kept per parse, in the context's
// bag for plugin state, so that a table's history never leaves a mark on the
// value a reader gets back.
function implicit(ctx: any): WeakSet<object> {
  return (ctx.u.toml_implicit ??= new WeakSet())
}

// A key about to be given a value it already has: `a = 1` then `a = 2`,
// `a.b = 1` then `a.b = 2`, `{b = 1, b = 2}`. TOML allows a key one value,
// and the base grammar's duplicate-key rule (last wins, tables merged) is
// not it. Raised on the key's own token, which is where the reader looks.
function redefines(container: any, key: string, r: any, ctx: any): void {
  const existing = container[key]
  if (undefined === existing) {
    return
  }

  throw new JsonicError(
    'toml_key_conflict',
    {
      key,
      why: `it already has the value ${JSON.stringify(existing)}`,
    },
    r.o0, r, ctx)
}

// A table node, or a DIAGNOSED refusal to descend into something that is not
// one, or to define a table a second time.
//
// TOML forbids redefining a key, so `a = {b = 1, b.c = 2}` and
// `a = 1` + `[a.b]` are invalid documents. Neither port said so. TypeScript
// walked into the scalar and JavaScript raised the assignment itself —
// "Cannot create property 'c' on number '1'" — an uncaught TypeError with no
// code, no position and no mention of TOML. AGENTS.md is explicit that this
// still counts as a rejection but NOT a conformant one ("a diagnosed
// rejection is a real parse error ... anything else is an internal crash"),
// and that turning a crash into a diagnosis is the point: it raises
// INVALID_DIAGNOSED_FLOOR.
//
// Every refusal is raised on the segment's own token, `r.o0`: every caller
// is the alternate action of the rule that matched the segment, so the key
// being redefined is in hand, and Go and Rust point at it too. These used to
// be raised on `ctx.t0`, which the engine has emptied by the time an
// alternate's action runs, so every header conflict read `1:1`.
function tableAt(
  container: any, key: string, r: any, ctx: any, how: string
): any {
  const existing = container[key]

  if (null == existing) {
    const made = (container[key] = node())
    if (DESCEND === how) {
      implicit(ctx).add(made)
    }
    return made
  }

  if (Array.isArray(existing)) {
    if (DEFINE !== how) {
      return existing
    }

    // `[[fruit.variety]]` then `[fruit.variety]`. Both ports used to accept
    // this and both DESTROYED data doing it, in opposite directions: this one
    // kept the array and silently dropped the second table's contents, Go
    // replaced the array with the second table and dropped the first.
    throw new JsonicError(
      'toml_key_conflict',
      { key, why: 'it is already an array of tables' },
      r.o0, r, ctx)
  }

  // An existing TABLE passes when walked through. Object.create(null) has no
  // prototype, so `instanceof` and `constructor` are both unavailable — hence
  // the shape test. A header may DEFINE it only if a header's prefix made it
  // and no header has defined it yet: `[a]` after `[a.b]` is that one case,
  // and `[a]` after `[a]`, after `a.b = 1` or after `a = {}` is a key defined
  // twice. Every port used to let all of those through.
  //
  // A dotted key that walks through a table defines it, as it defines every
  // table it creates, so the table leaves the implicit set and no header
  // defines it afterwards. Without that, `[x.a.b]`, `[x]` with `a.c = 1`, and
  // then `[x.a]` would consume a mark the dotted key had made stale. No
  // document reaches this today: a dotted key walks the map its own table
  // body or inline table is parsed into, which holds no header's tables, and
  // the body checks against its table (`bodyTable`) refuse that document at
  // `a.c`. It keeps this rule true without leaning on that.
  if ('object' === typeof existing) {
    if (DIVE === how) {
      implicit(ctx).delete(existing)
    }
    if (DEFINE !== how || implicit(ctx).delete(existing)) {
      return existing
    }

    throw new JsonicError(
      'toml_key_conflict',
      { key, why: 'it is already defined' },
      r.o0, r, ctx)
  }

  throw new JsonicError(
    'toml_key_conflict',
    {
      key,
      why: `it already has the value ${JSON.stringify(existing)}`,
    },
    r.o0, r, ctx)
}

// The table a table body is merged into, when `r` writes the TOP level of
// that body: a pair in the body's map, or the first segment of a dotted key
// there. A table's body is parsed into a map of its own (`@map-bo`) and
// merged into the table when the body ends (`@table-bc`), so a key the table
// already holds from an earlier header is not in the map the body's own
// checks read: `[a.b]` `c = 1` then `[a]` `b = 2` replaced `b`, losing `c`,
// and `[[a.b]]` then `[a]` `b.y = 2` replaced the array. Nothing writes to the
// table while its body is parsed, so checking a key against it here, where
// the key's token is in hand, is checking it at the merge, and the refusal
// points at the key, as every pair conflict does. Undefined for a pair or a
// dotted key in an inline table, whose map is the value itself, and for a
// later segment of a dotted key, which writes below the top level.
function bodyTable(r: any): any {
  const map = 'pair' === r.parent?.name ? r.parent.parent : r.parent
  return 'map' === map?.name && 'table' === map.parent?.name
    ? map.parent.node
    : undefined
}



// Date and time RANGE validation.
//
// Both ports matched date/time values on SHAPE alone — `^\d\d\d\d-\d\d-\d\d`
// says nothing about whether 13 is a month or 32 a day. Neither port noticed,
// and each mishandled the result in its own way. This one built a JS `Date`,
// which never fails: `1988-02-30` rolled over to `1988-03-01`, and
// `2006-01-32` became an Invalid Date that serialized to `null` — the value
// destroyed outright. Go kept the source text, so it round-tripped the
// impossible date back out unchanged. Twelve invalid documents, accepted by
// both, with 38 value disagreements between them.
//
// Range-checking here is what makes the two ports agree, because it removes
// the value they were disagreeing about rather than choosing between them.
const MONTH_DAYS = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]

function daysInMonth(year: number, month: number): number {
  if (2 !== month) {
    return MONTH_DAYS[month - 1]
  }
  // Proleptic Gregorian, the calendar RFC 3339 specifies. 2100 is NOT a leap
  // year, which is exactly what the corpus's feb-29 document tests.
  const leap = (0 === year % 4 && 0 !== year % 100) || 0 === year % 400
  return leap ? 29 : 28
}

// Seconds may be 60: RFC 3339 permits a positive leap second, and TOML
// inherits its date-time grammar from it. 61 is the corpus's second-over.
function timeInRange(hour: number, minute: number, second: number): boolean {
  return hour <= 23 && minute <= 59 && second <= 60
}

// The capture form of isodateRe. Anchored at BOTH ends so it can only agree
// with what that regex already matched; a mismatch here means the two have
// drifted apart, and the value is let through rather than silently rejected
// on a shape this function does not actually understand.
const ISODATE_PARTS =
  /^(\d{4})-(\d{2})-(\d{2})(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(?:[Zz]|[-+](\d{2}):(\d{2}))?)?$/

function isodateInRange(text: string): boolean {
  const p = text.match(ISODATE_PARTS)
  if (!p) {
    return true
  }

  const [year, month, day] = [p[1], p[2], p[3]].map((d) => parseInt(d, 10))
  if (month < 1 || 12 < month || day < 1 || daysInMonth(year, month) < day) {
    return false
  }

  // No time part: the date alone is in range.
  if (null == p[4]) {
    return true
  }
  if (!timeInRange(+p[4], +p[5], null == p[6] ? 0 : +p[6])) {
    return false
  }

  // Offset, when written as +hh:mm rather than Z.
  return null == p[7] || (+p[7] <= 23 && +p[8] <= 59)
}

const LOCALTIME_PARTS = /^(\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?$/

function localtimeInRange(text: string): boolean {
  const p = text.match(LOCALTIME_PARTS)
  if (!p) {
    return true
  }
  return timeInRange(+p[1], +p[2], null == p[3] ? 0 : +p[3])
}


const Toml: Plugin = (tn: Tabnas, _options: TomlOptions) => {
  // Human descriptions for TOML tokens, surfaced in railroad diagram legends
  // (read off the live config by @tabnas/railroad).
  tn.options({
    config: {
      modify: {
        'toml-tokendesc': (cfg: any) => {
          cfg.tokenDesc = Object.assign(cfg.tokenDesc || {}, {
            '#ID': 'bare key: letters, digits, _ or -',
          })
        },
      },
    },
  })

  // Named function references used by the declarative grammar.
  const refs: Record<string, any> = {
    // Options callbacks.
    '@make-toml-string-matcher': makeTomlStringMatcher,

    // Referenced by the embedded grammar's `val:` fields, used by the Go
    // port which keeps the regex-based matchers. The TS plugin replaces
    // the whole matcher with @isodate-match / @localtime-match below, so
    // these aren't actually invoked on this side — kept only so
    // jsonic.grammar() can resolve the '@isodate-val' / '@localtime-val'
    // refs during option installation.
    '@isodate-val': (res: any) => {
      const date: any = new Date(res[0])
      date.__toml__ = {
        kind:
          (null == res[4] ? 'local' : 'offset') +
          '-date' +
          (null == res[1] ? '' : '-time'),
        src: res[0],
      }
      return date
    },

    '@localtime-val': (res: any) => {
      const date: any = new Date(
        60 * 60 * 1000 + new Date('1970-01-01 ' + res[0]).getTime(),
      )
      date.__toml__ = { kind: 'local-time', src: res[0] }
      return date
    },

    // Context-aware replacements installed after jsonic.grammar() runs.
    // A bare date-shaped key like `2001-02-03 = 1` or a table header
    // `[2002-01-02]` would otherwise be swallowed by the regex value
    // matcher before the #ID token matcher ever gets a chance.
    '@isodate-match': (lex: Lex, rule: any) => {
      if (isKeyContext(lex, rule)) return null
      const m = lex.fwd.match(
        /^\d\d\d\d-\d\d-\d\d([Tt ]\d\d:\d\d(:\d\d(\.\d+)?)?([Zz]|[-+]\d\d:\d\d)?)?/,
      )
      if (!m) return null
      const pnt = lex.pnt
      if (!isodateInRange(m[0])) {
        return lex.bad('invalid_datetime', pnt.sI, pnt.sI + m[0].length)
      }
      const date: any = new Date(m[0])
      date.__toml__ = {
        kind:
          (null == m[4] ? 'local' : 'offset') +
          '-date' +
          (null == m[1] ? '' : '-time'),
        src: m[0],
      }
      const tkn = lex.token('#VL', date, m[0], pnt)
      pnt.sI += m[0].length
      pnt.cI += m[0].length
      return tkn
    },

    '@localtime-match': (lex: Lex, rule: any) => {
      if (isKeyContext(lex, rule)) return null
      const m = lex.fwd.match(/^\d\d:\d\d(:\d\d(\.\d+)?)?/)
      if (!m) return null
      const pnt = lex.pnt
      if (!localtimeInRange(m[0])) {
        return lex.bad('invalid_datetime', pnt.sI, pnt.sI + m[0].length)
      }
      const date: any = new Date(
        60 * 60 * 1000 + new Date('1970-01-01 ' + m[0]).getTime(),
      )
      date.__toml__ = { kind: 'local-time', src: m[0] }
      const tkn = lex.token('#VL', date, m[0], pnt)
      pnt.sI += m[0].length
      pnt.cI += m[0].length
      return tkn
    },

    // State actions (auto-applied by fnref via @<rule>-<state> convention).
    '@toml-bo': (r: Rule) => {
      r.node = node()
    },

    // Allocate this map's node up front, mirroring jsonic Go's
    // @map-bo-jsonic. The new core's @tabnas/json allocates a braced map's
    // node in the @object$ alt action (on #OB), but TOML prepends its own
    // map-open alts (e.g. the inline-table `{ s: ['#OB' '#ST #NR #ID'] p:
    // pair }`) that match first and carry no @object$, so without this the
    // pushed `pair` would inherit an undefined node and pairval would throw
    // reading r.node[key]. Allocating here gives every map (table-context
    // and inline-table alike) a real object; @table-bc copies the child map
    // into the table node, so a fresh node per map is consistent.
    '@map-bo': (r: Rule) => {
      r.node = node()
    },

    '@table-bo': (r: Rule) => {
      r.node = r.parent.node
    },

    // Merge the body into the table. Every key of the body was checked
    // against the table when the body defined it (`bodyTable`), so this
    // only ever adds keys and never replaces one.
    '@table-bc': (r: Rule) => {
      if (!r.u.top_dive) {
        Object.assign(r.node, r.child.node)
      }
    },

    '@table-ac': (_r: Rule, _ctx: any, next: any) => {
      next.n.table_dive = 0
      next.n.table_array = 0
    },

    '@dive-bc': (r: Rule, ctx: any) => {
      if (r.u.dive_end) {
        redefines(r.node, r.o0.val, r, ctx)
        r.node[r.o0.val] = r.child.node
      }
    },

    // Before the base grammar's own before-close action writes the pair:
    // a key already in the table is a conflict, not a merge, whether the
    // body has it or the table did before the body began.
    '@pair-bc/prepend': (r: Rule, ctx: any) => {
      if (r.u.pair) {
        redefines(r.node, r.u.key, r, ctx)
        const table = bodyTable(r)
        if (table) {
          redefines(table, r.u.key, r, ctx)
        }
      }
    },

    // Alt actions.
    '@table-dive-start': (r: any, ctx: any) => {
      let key = r.o0.val
      if (r.n.table_array && Array.isArray(r.parent.node[key])) {
        let arr = r.parent.node[key]
        let last = arr[arr.length - 1]
        r.node = last ? last : (arr.push(node()), arr[arr.length - 1])
      } else {
        r.node = tableAt(r.parent.node, key, r, ctx, DESCEND)
      }
    },

    '@table-dive-mid': (r: any, ctx: any) => {
      let key = r.o0.val
      if (Array.isArray(r.prev.node)) {
        let arr = r.prev.node
        let last = arr[arr.length - 1]
        last = last ? last : (arr.push(node()), arr[arr.length - 1])
        r.node = tableAt(last, key, r, ctx, DESCEND)
      } else {
        r.node = tableAt(r.prev.node, key, r, ctx, DESCEND)
      }
    },

    '@table-key-cs-head': (r: any, ctx: any) => {
      let key = r.o0.val
      r.node = r.n.table_array
        ? arrayAt(r.parent.node, key, r, ctx)
        : tableAt(r.parent.node, key, r, ctx, DEFINE)
    },

    '@table-key-cs-tail': (r: any, ctx: any) => {
      let key = r.o0.val
      let container = r.prev.node
      if (Array.isArray(container)) {
        // The segment before this one descended through an array of
        // tables, so the key is defined in its LAST table: as a table for
        // `[a.b.c]`, and as an array of tables for `[[a.b.c]]`, which is
        // how a `[[...]]` nests under a `[[...]]` with a dotted prefix.
        // This arm used to define a table whichever kind of header it was,
        // and @table-cs-push then refused that table as a key conflict, so
        // `[[a.b]]` followed by `[[a.b.c]]`, valid TOML, was rejected.
        let arr = container
        let last = arr[arr.length - 1]
        container = last ? last : (arr.push(node()), arr[arr.length - 1])
      }
      r.node = r.n.table_array
        ? arrayAt(container, key, r, ctx)
        : tableAt(container, key, r, ctx, DEFINE)
    },

    '@table-cs-push': (r: any, ctx: any) => {
      // `[[a]]` where `a` is already a scalar: .push is not a function, and
      // that was the raw TypeError. The array itself is produced by
      // arrayAt above, so reaching a non-array here means the key conflicts.
      if (!Array.isArray(r.prev.node)) {
        throw new JsonicError(
          'toml_key_conflict',
          {
            key: r.o0 ? r.o0.val : '',
            why: `it already has the value ${JSON.stringify(r.prev.node)}`,
          },
          ctx.t0, r, ctx)
      }
      r.prev.node.push((r.node = node()))
    },

    '@pair-key-set': (r: Rule) => {
      r.u.key = r.o0.val
    },

    // A dotted key's leading segment. At the top of a table body, the key
    // may not be one the table already holds: `[a.b.c]` `z = 9` then `[a]`
    // `b.c.t = 1` would replace `b` at the merge, losing `z`.
    '@dive-key-dot': (r: any, ctx: any) => {
      const table = bodyTable(r)
      if (table) {
        redefines(table, r.o0.val, r, ctx)
      }
      r.node = tableAt(r.parent.node, r.o0.val, r, ctx, DIVE)
    },

    // Conditions.
    '@table-top-dive-cond': (r: any) => 1 === r.d && 'table' !== r.prev.name,
    '@lte-table-dive': (r: any) => r.lte('table_dive'),
    '@lte-table-array-1': (r: any) => r.lte('table_array', 1),
    '@lte-dive-key-1': (r: any) => r.lte('dive_key', 1),
    '@lte-pk': (r: any) => r.lte('pk'),
    '@map-is-table-parent': (r: any) => 'table' === r.parent.name,

    // Conditional next-rule targets (p:/r: returning a rule name or false).
    '@table-end-p': (r: any) => !r.n.table_array && 'map',
    '@table-end-r': (r: any) => r.n.table_array && 'table',
  }

  // Parse embedded grammar definition using a jsonic-grammar engine,
  // then apply options and rules to this plugin's tabnas instance.
  const grammarDef: any = new Tabnas().use(jsonic).parse(grammarText)
  grammarDef.ref = refs

  // Patch option values that can't be expressed in the grammar text
  // (NaN/Infinity literals can't round-trip through Jsonic parsing).
  grammarDef.options.value = {
    def: {
      nan: { val: NaN },
      '+nan': { val: NaN },
      '-nan': { val: NaN },
      inf: { val: Infinity },
      '+inf': { val: Infinity },
      '-inf': { val: -Infinity },
    },
  }

  tn.grammar(grammarDef)

  // A TOML document may begin with a UTF-8 BOM (U+FEFF), which the spec
  // says to ignore (BurntSushi/toml-test valid/utf8-bom-01, -02). The
  // engine's lexer has no BOM concept, so install a matcher that eats a
  // single leading BOM as an ignorable #SP token. It runs before the
  // `match` matcher (order 1e6) and only fires at source index 0, so a
  // BOM anywhere else is still the error it should be. The Go port does
  // the same via registerBOMMatcher.
  tn.options({
    // A code with no template renders as "unknown error: toml_key_conflict",
    // which tells the author nothing and would make the diagnosis this
    // change adds barely better than the TypeError it replaces.
    error: {
      toml_key_conflict:
        'cannot define {key}, {why}',
      invalid_datetime:
        'date or time is out of range',
    },
    hint: {
      toml_key_conflict: `
TOML does not allow a key to be redefined, and a key that already holds a
value is not a table you can add to. This usually means the same name was
used twice: a key given a second value, a table or table-array header over a
key that already holds a value, a header written a second time, a header for
a table that a dotted key or an inline table had already defined, or the same
name twice inside one inline table.`,
      invalid_datetime: `
The value has the shape of a date or time, but one of its components is out
of range: month 1-12, day 1 to the length of that month, hour 0-23, minute
and second 0-59 (a second may be 60, for a leap second), and the same limits
again for a +hh:mm offset. February is checked against the actual year, so
2100-02-29 is rejected - 2100 is not a leap year.`,
    },
    lex: {
      match: {
        bom: { order: 5e5, make: makeBomMatcher },
      },
    },
  })

  // Swap the grammar's regex-based date/time matchers for the
  // context-aware function matchers. The grammar file keeps the regex
  // form so the Go port (which has no equivalent of isKeyContext) still
  // parses it. On the TS side, these overrides let date-shaped bare keys
  // fall through to the #ID token matcher.
  tn.options({
    match: {
      value: {
        isodate: { match: refs['@isodate-match'] },
        localtime: { match: refs['@localtime-match'] },
      },
    },
  })
}

// Byte order mark. A TOML file is allowed to start with one; it carries
// no content and must not reach the parser.
const BOM = '\uFEFF'

// Lex matcher factory for the leading BOM. Emits a zero-value #SP token
// (#SP is in the IGNORE token set, so the parser never sees it).
function makeBomMatcher() {
  return function bomMatcher(lex: Lex) {
    const pnt = lex.pnt
    if (0 !== pnt.sI || BOM !== lex.src[0]) {
      return undefined
    }
    const tkn = lex.token('#SP', undefined, BOM, pnt)
    pnt.sI += 1
    pnt.cI += 1
    return tkn
  }
}

// Value matchers fire unconditionally, so a date-shaped bare key
// (e.g. `2001-02-03 = 1`, `[2002-01-02]`, `a.2001-02-08 = 7`) would be
// claimed as a datetime value unless we defer to the #ID token matcher
// when the current rule position accepts a key. Value-producing rules
// (val, list, elem) never list #ID in their expected tokens; key-accepting
// rules (toml, map, dive, pair, table) do. Value matchers aren't told which
// tI they're at, so we scan all positions in the current state.
function isKeyContext(lex: Lex, rule: any): boolean {
  const tcol = rule?.spec?.def?.tcol
  if (!tcol) return false
  const oc = 'o' === rule.state ? 0 : 1
  const positions = tcol[oc]
  if (!positions) return false
  const idTin = lex.tokenize('#ID')
  for (const expected of positions) {
    if (expected && expected.includes(idTin)) return true
  }
  return false
}

// Adapted from https://github.com/huan231/toml-nodejs/blob/master/src/tokenizer.ts
// Copyright (c) 2022 Jan Szybowski, MIT License
function makeTomlStringMatcher() {
  return function stringMatcher(lex: Lex) {
    let { pnt, src } = lex
    let { sI, rI, cI } = pnt
    let srclen = src.length

    let isMultiline = false
    let begin = sI

    let delimiter = src[sI]
    let singleQuote = "'" === delimiter
    let doubleQuote = '"' === delimiter

    if (!singleQuote && !doubleQuote) {
      return
    }

    if (delimiter === src[sI + 1]) {
      if (delimiter !== src[sI + 2]) {
        pnt.sI = sI + 2
        pnt.cI = cI + 2
        return lex.token('#ST', EMPTY, EMPTY, pnt)
      }

      sI += 2
      cI += 2
      isMultiline = true
    }

    // A newline immediately following the opening delimiter will be trimmed.
    // https://toml.io/en/v1.0.0#string
    if (isMultiline) {
      if ('\n' === src[sI + 1]) {
        ++sI
        cI = 0
      }
    }

    let value = ''

    // `closed` records that the loop stopped ON the closing delimiter, which
    // is the only legitimate way out. Without it, the exit below cannot tell a
    // terminated string from a source that simply ran out — and it answered
    // both with a token. See the comment there.
    let closed = false

    // `sI < srclen`, not `srclen - 1`. The old bound stopped one character
    // short, so a string whose closing delimiter is the final character of the
    // source was never examined, and `case undefined` below was unreachable.
    for (; sI < srclen; ) {
      ++sI
      ++cI

      const char = src[sI]

      switch (char) {
        case '\n':
          if (!isMultiline) {
            return lex.bad('unprintable', sI, sI + 1)
          }

          value += char
          cI = 0
          ++rI
          continue

        case delimiter:
          if (isMultiline) {
            if (delimiter !== src[sI + 1]) {
              value += delimiter
              continue
            }

            if (delimiter !== src[sI + 2]) {
              value += delimiter
              value += delimiter
              cI += 1
              sI += 1
              continue
            }

            cI += 2
            sI += 2

            if (delimiter === src[sI + 1]) {
              value += delimiter
              sI++
            }

            if (delimiter === src[sI + 1]) {
              value += delimiter
              sI++
            }
          }

          ++cI
          ++sI

          closed = true
          break

        case undefined:
          return lex.bad('unterminated_string', begin, sI)

        default:
          if (sI >= srclen) {
            return lex.bad('unterminated_string', begin, sI)
          }

          if (
            !isUnicodeCharacter(char) ||
            isControlCharacterOtherThanTab(char)
          ) {
            return lex.bad('unprintable', sI, sI + 1)
          }

          switch (delimiter) {
            case "'":
              value += char

              continue

            case '"':
              if (char === '\\') {
                const char = src[(++cI, ++sI)]

                if (isEscaped(char)) {
                  value += ESCAPES[char]

                  continue
                } else if (char === 'x') {
                  sI++
                  // parseInt is NOT a validator: it stops at the first
                  // non-hex character and returns what it read so far, so
                  // `\xAg` decoded as 0x0A and swallowed the `g`, and `\xA"`
                  // decoded as 0x0A and swallowed the CLOSING QUOTE — which
                  // then surfaced as unterminated_string rather than a bad
                  // escape. Go's strconv.ParseInt errors on both, which is
                  // why it rejected them and this did not. Same defect the
                  // engine had at its own \x / \u sites.
                  const xs = src.substring(sI, sI + 2)
                  let cc = /^[0-9a-fA-F]{2}$/.test(xs) ? parseInt(xs, 16) : NaN

                  if (isNaN(cc)) {
                    sI = sI - 2
                    cI -= 2
                    pnt.sI = sI
                    pnt.cI = cI
                    return lex.bad('invalid_ascii', sI, sI + 4)
                  }

                  let us = String.fromCharCode(cc)

                  value += us
                  sI += 1 // Loop increments sI.
                  cI += 2

                  continue
                }
                // Any Unicode character may be escaped
                // with the \uXXXX or \UXXXXXXXX forms.
                // The escape codes must be valid Unicode scalar values.
                // https://toml.io/en/v1.0.0#string
                else if (char === 'u' || char === 'U') {
                  let beginUnicode = sI
                  const size = char === 'u' ? 4 : 8

                  let codePoint = ''

                  for (let i = 0; i < size; i++) {
                    const char = src[(++cI, ++sI)]

                    if (sI >= srclen || !isHexadecimal(char)) {
                      return lex.bad('invalid_unicode', beginUnicode, sI)
                    }

                    codePoint += char
                  }

                  // Range-checked BEFORE the code point is built.
                  // String.fromCodePoint THROWS a RangeError above 0x10FFFF,
                  // so `\UFFFFFFFF` left this matcher as an uncaught internal
                  // error and reached the caller as `unexpected` — a crash
                  // wearing a diagnostic's clothes. isUnicodeCharacter below
                  // could never have caught it: it never ran.
                  const cp = parseInt(codePoint, 16)

                  if (!(cp <= 0x10ffff)) {
                    return lex.bad('invalid_unicode', beginUnicode, sI)
                  }

                  const result = String.fromCodePoint(cp)

                  if (!isUnicodeCharacter(result)) {
                    return lex.bad('invalid_unicode', beginUnicode, sI)
                  }

                  value += result

                  continue
                }

                // For writing long strings without introducing
                // extraneous whitespace, use a "line ending
                // backslash".  When the last non-whitespace character
                // on a line is an unescaped \, it will be trimmed
                // along with all whitespace (including newlines) up
                // to the next non-whitespace character or closing
                // delimiter.
                // https://toml.io/en/v1.0.0#string
                if (
                  isMultiline &&
                  (isWhitespace(char) || char === '\n' || char === '\r')
                ) {
                  while (
                    (' ' === src[sI + 1] && ++cI) ||
                    ('\t' === src[sI + 1] && ++cI) ||
                    ('\n' === src[sI + 1] && ((cI = 0), ++rI)) ||
                    ('\r' === src[sI + 1] &&
                      '\n' === src[sI + 2] &&
                      ((cI = 0), ++sI, ++rI))
                  ) {
                    sI++
                  }

                  continue
                }

                value += '\u001b'
                continue
              }

              value += char
              continue
          }
      }

      break
    }

    // Reaching here without the closing delimiter means the source ran out
    // mid-string. This used to fall straight through to the token below, so an
    // unterminated string became a VALID #ST holding whatever had been read —
    // `a = "abc ` parsed to {a: "abc "}, with a closing quote the source never
    // had. It was recorded as a mere error-code divergence (TypeScript
    // `unexpected` vs Go `unterminated_string`), which hid it: the `unexpected`
    // only appeared when the truncated token happened to leave a stray
    // character behind, and when it did not, malformed TOML parsed silently.
    // Go has always had this raise after its loop; this is the port catching up
    // with it, in the direction the port was right.
    if (!closed) {
      return lex.bad('unterminated_string', begin, sI)
    }

    pnt.sI = sI
    pnt.cI = cI
    pnt.rI = rI

    let st = lex.token('#ST', value, src.substring(begin, sI), pnt)

    return st
  }
}

const ESCAPES = {
  b: '\b',
  t: '\t',
  n: '\n',
  f: '\f',
  r: '\r',
  '"': '"',
  '\\': '\\',
}

const isEscaped = (char: string): char is keyof typeof ESCAPES => {
  return char in ESCAPES
}

const isUnicodeCharacter = (char: string) => {
  // Compare by code point, not UTF-16 lexicographic order: '\u{10ffff}'
  // encodes as the surrogate pair '􏿿', so a literal <= compare
  // wrongly rejects single code units in [, \udbff). It also must
  // accept lone surrogates so characters outside the BMP (iterated as two
  // code units) don't trip an "unprintable" error.
  return (char.codePointAt(0) ?? 0) <= 0x10ffff
}

const isControlCharacter = (char: string) => {
  return ('\u{0}' <= char && char < '\u{20}') || char === '\u{7f}'
}

const isControlCharacterOtherThanTab = (char: string) => {
  return isControlCharacter(char) && char !== '\t'
}

export const isHexadecimal = (char: string) => {
  return (
    ('A' <= char && char <= 'Z') ||
    ('a' <= char && char <= 'z') ||
    ('0' <= char && char <= '9')
  )
}

const isWhitespace = (char: string) => {
  return char === ' ' || char === '\t'
}

// Default option values.
Toml.defaults = {} as TomlOptions

// VERSION is this package's version. It MUST equal package.json "version":
// the release orchestrator rewrites both, and test/version.test.ts fails the
// build if they drift. Mirrors `const VERSION` in go/toml.go.
const VERSION = '0.5.10'

export { Toml, VERSION }

export type { TomlOptions }

export { translate } from './translate'
export type { TranslationPart, TranslationParts } from './translate'
