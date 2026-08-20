/* Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License */

import { test, describe } from 'node:test'
import Fs from 'node:fs'
import Path from 'node:path'
import { execFileSync } from 'node:child_process'
import { deepStrictEqual as equal, ok } from 'node:assert/strict'

import { loadSpec } from '@tabnas/support'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Toml } from '..'


// BurntSushi/toml-test conformance harness.
//
//   upstream: https://github.com/BurntSushi/toml-test
//   pinned:   9eef1b959e0449d41a31d4e4e0a839faee534b36
//
// The corpus is NOT committed (project rule: no vendored third-party test
// corpora). `scripts/fetch-toml-test.sh` clones it, pinned to that exact
// commit, into the gitignored `ts/test/toml-test/`. `npm test` runs the
// fetch via the `pretest` hook, so CI has the corpus; ensureCorpus() below
// re-runs the same script as a belt-and-braces fallback.
//
// THIS SUITE MUST NEVER SKIP. It used to `t.skip()` when the corpus was
// absent — which is exactly what CI looked like — so the conformance test
// had never executed on CI at all while reporting green. A conformance
// suite that quietly does not run is worse than no suite, because the
// green tick is a lie. If the corpus cannot be obtained, FAIL.
//
// Both halves are now exercised:
//
//   valid/    must parse AND produce the correct value. Asserted at 100%.
//   invalid/  must be rejected. The parser does not reject them all yet, so
//             this is asserted against an EXACT measured count kept in
//             test/conformance.tsv — one file, both runtimes. See that
//             file's header for why exact and not a floor.

const SUITE_URL = 'https://github.com/BurntSushi/toml-test'
const SUITE_PIN = '9eef1b959e0449d41a31d4e4e0a839faee534b36'

const REPO_ROOT = Path.join(__dirname, '..', '..')
const SUITE_ROOT = Path.join(REPO_ROOT, 'ts', 'test', 'toml-test')
const FETCH_SCRIPT = Path.join(REPO_ROOT, 'scripts', 'fetch-toml-test.sh')


// Guarantee the corpus is on disk. Never skips: if it cannot be obtained,
// throw with an actionable message so the suite goes red.
function ensureCorpus(): string {
  const halves = ['valid', 'invalid'].map((h) => Path.join(SUITE_ROOT, 'tests', h))
  const present = () => halves.every((p) => Fs.existsSync(p))

  if (!present()) {
    try {
      execFileSync('bash', [FETCH_SCRIPT], { stdio: 'inherit' })
    }
    catch (e: any) {
      throw new Error(
        'BurntSushi/toml-test conformance corpus is MISSING and could not be fetched.\n' +
        `  suite:  ${SUITE_URL} @ ${SUITE_PIN}\n` +
        `  expect: ${halves.join('\n          ')}\n` +
        `  fix:    bash ${FETCH_SCRIPT}\n` +
        'This test deliberately FAILS rather than skipping — a conformance\n' +
        'test that silently does not run reports a green tick that is a lie.\n' +
        `  cause:  ${e && e.message}`)
    }
  }

  if (!present()) {
    throw new Error(
      `toml-test corpus still absent after running: bash ${FETCH_SCRIPT}\n` +
      `Expected ${halves.join(' and ')}.`)
  }

  return SUITE_ROOT
}


describe('toml', () => {

  // A TOML document may start with a UTF-8 BOM, which is ignored; a BOM
  // anywhere else is an error. Covered by BurntSushi/toml-test
  // valid/utf8-bom-01 and -02, which only run when that (optional) suite
  // is installed — hence this local guard. Go's twin is TestLeadingBOM in
  // go/features_test.go.
  test('leading-bom', () => {
    const toml = new Tabnas().use(jsonic).use(Toml)
    const BOM = '\uFEFF'

    // Round-tripped through JSON before comparing: TOML nodes carry no
    // prototype (the core's convention, and what keeps a table named
    // __proto__ from reaching Object.prototype), and deepStrictEqual
    // compares prototypes. The .tsv suites already normalize this way.
    const norm = (v: any) => JSON.parse(JSON.stringify(v))

    equal(norm(toml.parse(BOM + 'a = 1')), { a: 1 })
    equal(norm(toml.parse(BOM + '# c\na = 1')), { a: 1 })

    let mid: any = null
    try {
      toml.parse('a = 1\n' + BOM + 'b = 2')
    }
    catch (e: any) {
      mid = e
    }
    equal(null != mid, true, 'BOM after the first character must not parse')
  })

  test('toml-valid', async (_t) => {
    const toml = new Tabnas().use(jsonic).use(Toml)

    let root = Path.join(ensureCorpus(), 'tests', 'valid')

    let found = find(root, [])

    let fails: any[] = []
    let counts = { pass: 0, fail: 0 }
    for (let test of found) {
      try {
        // console.log('TEST', test.name)
        test.out = toml.parse(test.toml)
        test.norm = norm(test.out, test.name)
        equal(test.norm, test.json)
        // console.log('PASS', test.name)
        counts.pass++
      }
      catch (e: any) {
        counts.fail++
        fails.push(Path.relative(root, test.name) + ': ' + firstLine(e.message))
      }
    }

    console.log('COUNTS', counts)

    // The whole point of running the upstream suite is that a
    // non-conformance breaks the build. Report every failure, then fail.
    equal(fails, [],
      `BurntSushi/toml-test valid suite: ${counts.fail} of ` +
      `${found.length} failed:\n  ` + fails.join('\n  '))

    // Guard against the suite silently emptying out (a bad clone, a
    // moved directory): a green run must have actually run the fixtures.
    if (counts.pass < 200) {
      throw new Error(
        'toml-test valid suite looks truncated: only ' + counts.pass +
        ' fixtures ran')
    }


    // Handle test case oddities
    function norm(val: any, rawName: string) {
      // The fixture name arrives as a filesystem path, so on Windows its
      // separator is `\`. Every name test below is written with `/`, so
      // without this every fixup keyed on a two-segment name silently
      // stopped matching there and the fixture failed. That is exactly
      // what happened the first time this suite reached a Windows runner:
      // the 7 failures were precisely the 7 slash-bearing keys
      // (float/max-int, float/exponent, float/exponent-upper, float/zero,
      // inline-table/spaces, spec-1.0.0/float-0, spec-1.1.0/common-23),
      // while the slash-free `long` and `underscore` keys kept working.
      // Compare on one canonical separator.
      const name = rawName.split(Path.sep).join('/')

      // Tests where every numeric leaf is a float (values happen to be
      // integer-valued, so the int-vs-float guess below can't recover this
      // without help).
      const allFloat =
        name.endsWith('float/max-int') ||
        name.endsWith('spec-1.0.0/float-0') ||
        name.endsWith('spec-1.1.0/common-23') ||
        name.endsWith('inline-table/spaces') ||
        // Every leaf in these two is a float; an integer-valued exponent
        // (3e2, 3E2) parses to a plain JS number indistinguishable from an
        // integer, so route the whole fixture through goFloat(). Mirrors
        // the same list in go/toml_valid_test.go normalizeForToml().
        name.endsWith('float/exponent') ||
        name.endsWith('float/exponent-upper')

      let jstr = JSON.stringify(val, function(this: any, k: string, v: any) {
        if (Infinity === v) {
          v = '__toml__,float,inf'
        }
        else if (-Infinity === v) {
          v = '__toml__,float,-inf'
        }
        else if (Number.isNaN(v)) {
          v = '__toml__,float,nan'
        }
        // JSON can't round-trip -0, so tag it before it's serialised
        // (only matters when the test expects a float "-0"; an integer -0
        // normalises to "0" anyway).
        else if (Object.is(v, -0) && name.endsWith('float/zero')) {
          v = '__toml__,float,-0'
        }
        else if (this) {
          if (this[k]) {
            if (this[k].__toml__) {
              v = '__toml__,' + this[k].__toml__.kind + ',' + this[k].__toml__.src
            }
          }
        }

        return v
      })

      let jout = JSON.parse(jstr,
        (_k: string, v: any) => {
          let vt = typeof v
          if ('number' === vt) {
            if (name.endsWith('float/zero')) {
              // JS collapses -0 to "0"; the TOML test expects the sign.
              return { type: 'float', value: Object.is(v, -0) ? '-0' : '' + v }
            }
            else if (allFloat) {
              return { type: 'float', value: goFloat(v) }
            }
            else if (name.endsWith('long') &&
              v > 9e10) {
              return { type: 'integer', value: '9223372036854775807' }
            }
            else if (name.endsWith('long') &&
              v < -9e10) {
              return { type: 'integer', value: '-9223372036854775808' }
            }
            else if (name.endsWith('underscore') &&
              300000000000000 === v) {
              return { type: 'float', value: '3.0e14' }
            }
            else
              if (('' + v).match(/^-?[0-9]+$/)) {
                return { type: 'integer', value: '' + v }
              }
              else {
                return { type: 'float', value: '' + v }
              }
          }
          else if ('string' === vt) {
            if (v.startsWith('__toml__')) {
              let m = v.match(/__toml__,([^,]+),(.*)/)
              return {
                type: (({
                  'offset-date-time': 'datetime',
                  'local-date-time': 'datetime-local',
                  'local-date': 'date-local',
                  'local-time': 'time-local',
                } as any)[m[1]]) || m[1],
                value: m[2]
                  .replace(/t/g, 'T')
                  .replace(/ /g, 'T')
                  .replace(/z/g, 'Z')
                  .replace(/\.6Z/, '.600Z')
                  .replace(/\.6\+/, '.600+')
                  .replace(/^(\d\d:\d\d)$/, '$1:00')
                  .replace(/T(\d\d:\d\d)([-Z])/, 'T$1:00$2')
                  .replace(/T(\d\d:\d\d)$/, 'T$1:00')
              }
            }
            return { type: 'string', value: '' + v }
          }
          else if ('boolean' === vt) {
            return { type: 'bool', value: '' + v }
          }
          else if (null != v && 'object' == vt) {
            if (v.ten) {
              // 1e3 is not a float dude!
              if ('integer' === v.ten.type && '1000' === v.ten.value) {
                v.ten.type = 'float'
                v.ten.value = '1000.0'
              }
            }
            return v
          }

          return v
        })

      // console.log(jstr)

      return jout
    }
  })


  // The other half of the corpus: 509 documents that a TOML parser MUST
  // reject. This half had never been loaded by either runtime — the
  // must-fail files have been on disk since the first clone and nothing
  // read them.
  //
  // It is asserted against a FLOOR rather than at 100%, because the
  // parser does not reject them all today (the base grammar inherited
  // from @tabnas/jsonic is lenient: `x = tru` becomes a string, `a = 1
  // b = 2` parses without a newline, duplicate keys silently overwrite).
  // A floor is what ratchets: it fails the build the moment rejection
  // regresses, and it is meant to be raised — never lowered — as the
  // grammar tightens.
  test('toml-invalid', async (_t) => {
    const toml = new Tabnas().use(jsonic).use(Toml)

    const root = Path.join(ensureCorpus(), 'tests', 'invalid')
    const found = findInvalid(root, [])

    // Guard against the corpus silently emptying out (a bad clone, a
    // moved directory): a green run must have actually run the fixtures.
    ok(found.length >= CONFORMANCE.total,
      `toml-test invalid suite looks truncated: ${found.length} fixtures ` +
      `found, expected at least ${CONFORMANCE.total}`)

    let rejected = 0
    let diagnosed = 0
    const accepted: string[] = []
    const crashRejects: string[] = []

    for (const t of found) {
      let out: any
      try {
        out = toml.parse(t.toml)
      }
      catch (e: any) {
        // Rejected. Record HOW: a Tabnas/jsonic parse error carries a
        // string `code`; anything else is an internal crash. A crash is
        // still a rejection, but it is not a conformant one, so the two
        // are counted separately and neither can be traded for the other.
        rejected++
        if (e && 'string' === typeof e.code) {
          diagnosed++
        }
        else {
          crashRejects.push(
            `${t.name}: ${e && e.constructor && e.constructor.name}: ` +
            firstLine('' + (e && e.message)))
        }
        continue
      }
      accepted.push(`${t.name}: wrongly accepted as ${JSON.stringify(out)}`)
    }

    console.log(
      `toml-invalid: rejected ${rejected}/${found.length} ` +
      `(${((100 * rejected) / found.length).toFixed(1)}%), ` +
      `of which diagnosed ${diagnosed}, internal crash ${crashRejects.length}; ` +
      `wrongly accepted ${accepted.length}`)

    for (const line of crashRejects.slice(0, MAX_REPORT)) {
      console.log('  CRASH-REJECT ' + line)
    }
    if (crashRejects.length > MAX_REPORT) {
      console.log(`  ...and ${crashRejects.length - MAX_REPORT} more crash-rejections`)
    }
    for (const line of accepted.slice(0, MAX_REPORT)) {
      console.log('  ACCEPTED ' + line)
    }
    if (accepted.length > MAX_REPORT) {
      console.log(`  ...and ${accepted.length - MAX_REPORT} more wrongly accepted`)
    }

    // Exact, not a floor. A floor absorbs degradation silently: the
    // diagnosed floor this replaces sat 11 below its own measured value, so
    // eleven documents could have decayed from a diagnosed error into an
    // internal crash with the build still green.
    const where = `(suite ${SUITE_URL} @ ${SUITE_PIN}, ` +
      'counts in test/conformance.tsv)'

    equal(rejected, CONFORMANCE.rejected,
      `BurntSushi/toml-test invalid suite MOVED: ${rejected} of ` +
      `${found.length} rejected, test/conformance.tsv says ` +
      `${CONFORMANCE.rejected} ${where}. Fewer means documents that must be ` +
      'rejected are now accepted — do not edit the file to make this pass. ' +
      'More means the grammar improved: re-measure BOTH runtimes and update ' +
      'that one file.')

    equal(diagnosed, CONFORMANCE.diagnosed,
      `BurntSushi/toml-test invalid suite MOVED: ${diagnosed} of ` +
      `${found.length} rejections are diagnosed parse errors, ` +
      `test/conformance.tsv says ${CONFORMANCE.diagnosed} ${where}. A ` +
      'rejection that is really an internal crash does not count as ' +
      'conformance.')

    equal(rejected - diagnosed, CONFORMANCE.crashes,
      `BurntSushi/toml-test invalid suite MOVED: ${rejected - diagnosed} ` +
      `rejections are internal crashes, test/conformance.tsv says ` +
      `${CONFORMANCE.crashes} ${where}.`)
  })

  // Key conflicts are DIAGNOSED, not crashes.
  //
  // TOML forbids redefining a key, and a key that already holds a value is
  // not a table you can descend into or an array you can append to. This port
  // used to walk in anyway and let JavaScript raise the assignment itself:
  // "Cannot create property 'c' on number '1'", or "r.prev.node.push is not a
  // function" for the array-of-tables form. Uncaught TypeErrors — no code, no
  // position, no mention of TOML.
  //
  // AGENTS.md counts those as rejections but NOT conformant ones ("a
  // diagnosed rejection is a real parse error ... anything else is an
  // internal crash"), and says turning a crash into a diagnosis is the point.
  // It is what raised the diagnosed count in test/conformance.tsv.
  //
  // TS-LOCAL rather than a shared .tsv row, on purpose. The Go port still
  // ACCEPTS these documents and cannot yet reject them with this code: its
  // engine converts every action panic into an `internal` error by design
  // (parser/go/parser.go, "parsing never panics, whatever the input"), so the
  // check has to move into a grammar CONDITION before the two ports can share
  // a row. Named in the PR as follow-up; pinning a row both ports cannot pass
  // would just be a red build.
  test('key-conflict-is-diagnosed', () => {
    const toml = new Tabnas().use(jsonic).use(Toml)
    const norm = (v: any) => JSON.parse(JSON.stringify(v))

    // Each of these crashed with an uncaught TypeError before the repair.
    const conflicts = [
      'a = {b = 1, b.c = 2}',
      'a = {b = "s", b.c = 2}',
      'a = 1\n[a.b]\nc = 2',
      'a = 1\n[[a]]\nb = 2',
      '[a]\nb = 1\n[[a.b]]\nc = 2',

      // Redefining an array-of-tables as a table. These did NOT crash: both
      // ports accepted them and both silently DESTROYED data, in opposite
      // directions — this one kept the array and dropped the second table's
      // contents entirely, Go replaced the array with the second table and
      // dropped the first. Silent data loss on an invalid document is worse
      // than the TypeError above, because nothing at all reports it.
      // corpus: array/tables-02, table/duplicate-key-07.
      '[[fruit]]\nname = "apple"\n[[fruit.variety]]\n' +
      'name = "red delicious"\n[fruit.variety]\nname = "granny smith"',
      '[[x]]\na = 1\n[x]\nb = 2',
    ]
    for (const src of conflicts) {
      let caught: any = null
      try {
        toml.parse(src)
      }
      catch (e: any) {
        caught = e
      }

      ok(null != caught, `${JSON.stringify(src)}: expected a rejection`)
      equal(caught.code, 'toml_key_conflict',
        `${JSON.stringify(src)}: rejected as ` +
        `${caught.code ?? caught.constructor.name} — a rejection carrying no ` +
        'code is the uncaught crash this replaced, not a diagnosis')
    }

    // Controls. Descending into an existing TABLE, or into the last element
    // of an existing array-of-tables, is legitimate and must NOT read as a
    // conflict: four valid corpus documents do exactly this, and a first cut
    // of the check rejected all four.
    const allowed: [string, any][] = [
      ['a = {b = 1, c = 2}', { a: { b: 1, c: 2 } }],
      ['a = {b.c = 1, b.d = 2}', { a: { b: { c: 1, d: 2 } } }],
      ['[[x]]\ny = 1\n[x.z]\nw = 2', { x: [{ y: 1, z: { w: 2 } }] }],
    ]
    for (const [src, want] of allowed) {
      equal(norm(toml.parse(src)), want,
        `${JSON.stringify(src)} must still parse`)
    }
  })

  // Error COLUMNS after a non-ASCII character in a string.
  //
  // This port counts UTF-16 code units, which is what `col` means here.
  // The GO port brings its own string matcher, whose scan loop walks the
  // source a byte at a time, and it incremented the column once per BYTE:
  // a 2-byte `é` charged two columns and an astral character four, so
  // every diagnostic after a non-ASCII character in a string pointed past
  // where the problem was. Found by the fleet parity probe, which
  // reported Go's `col` running ahead of this port's by exactly the extra
  // bytes.
  //
  // go/strmatcher_col_test.go asserts the same five inputs. The astral
  // rows are the only ones where the two answers differ, and that
  // difference is the recorded engine divergence: this port counts UTF-16
  // units (an astral character is 2), Go counts runes (1). See
  // parser/DIVERGENCE.md, "Column positions for astral characters".
  test('error columns count characters, not bytes', () => {
    const cases: [string, string, number, number][] = [
      // Control: pure ASCII, where every unit coincides. Without it,
      // "columns count characters" is also satisfied by never counting.
      ['ascii', '[a b]', 2, 2],

      // 2 and 3 bytes, 1 rune, 1 UTF-16 unit: both ports agree.
      ['latin1', '["\u00e9" 1]', 5, 5],
      ['bmp', '["\u20ac" 1]', 5, 5],

      // 4 bytes, 1 rune, TWO UTF-16 units: the recorded divergence, and
      // the only rows where the two halves differ.
      ['astral', '["\u{1F600}" 1]', 6, 5],
      ['mixed', '["ab\u{1F600}cd" 1]', 10, 9],
    ]

    for (const [label, src, col, go] of cases) {
      const t = new Tabnas().use(jsonic).use(Toml)
      let err: any = null
      try {
        t.parse(src)
      }
      catch (e) {
        err = e
      }
      ok(null != err, `${label}: ${JSON.stringify(src)} parsed, expected a diagnostic`)

      // Read the SERIALISED diagnostic, not the thrown object: `col` is
      // part of the JSON contract (schema/diagnostic.schema.json) and is
      // not an own enumerable property of the error, so `err.col` is
      // `undefined` and an assertion against it would compare nothing.
      const diag = JSON.parse(JSON.stringify(err))
      equal(diag.col, col,
        `${label}: ${JSON.stringify(src)} col — Go says ${go}`)
    }
  })
})


// The invalid-half conformance counts, read from `test/conformance.tsv` at
// the repo root — the SAME file `go/toml_valid_test.go` reads, through the
// same @tabnas/support loader.
//
// They used to be two constants here and two more in the Go suite, all four
// commented "MEASURED on 2026-08-09" against the same pinned corpus, reading
// 227/212 and 230/230, with nothing comparing them. See that file's header
// for why they are now exact rather than floors, and for what the 9-document
// gap between the two runtimes is.
const CONFORMANCE = readConformance('ts')

function readConformance(runtime: string) {
  const spec = loadSpec(Path.join(findRepoRoot(), 'test', 'conformance.tsv'))
  const row = spec.rows.find((r: any) => runtime === r.named('runtime'))

  // A missing row must fail loudly. Defaulting to zero here would make every
  // assertion below trivially true, which is the shape of bug this whole
  // file exists to catch.
  if (!row) {
    throw new Error(
      `test/conformance.tsv has no row for runtime ${JSON.stringify(runtime)}`)
  }

  const num = (name: string) => {
    const raw = row.named(name)
    const val = Number(raw)
    if (!Number.isInteger(val)) {
      throw new Error(
        `test/conformance.tsv: ${runtime}.${name} is ` +
        `${JSON.stringify(raw)}, expected an integer`)
    }
    return val
  }

  return {
    total: num('total'),
    rejected: num('rejected'),
    diagnosed: num('diagnosed'),
    crashes: num('crashes'),
  }
}

// Walk up from this file — `dist-test/` at runtime — to the repo root, the
// same way findSpecDir does, so moving the suite does not mean recounting
// `..` hops.
function findRepoRoot(): string {
  let dir = __dirname
  for (let up = 0; up < 8; up++) {
    if (Fs.existsSync(Path.join(dir, 'test', 'conformance.tsv'))) {
      return dir
    }
    dir = Path.dirname(dir)
  }
  throw new Error('cannot find repo root (no test/conformance.tsv above ' +
    __dirname + ')')
}

// How many individual failures to print; the assertion message is not
// truncated. Only bounds console noise.
const MAX_REPORT = 40


// The invalid half has no companion .json — the documents are simply
// required not to parse.
function findInvalid(
  base: string,
  found: { name: string, toml: string }[],
  dir: string = base,
) {
  for (const file of Fs.readdirSync(dir).sort()) {
    const filepath = Path.join(dir, file)
    const desc = Fs.lstatSync(filepath)
    if (desc.isDirectory()) {
      findInvalid(base, found, filepath)
    }
    else if (desc.isFile() && file.endsWith('.toml')) {
      found.push({
        name: Path.relative(base, filepath).split(Path.sep).join('/'),
        toml: Fs.readFileSync(filepath).toString(),
      })
    }
  }
  return found
}

// Format a JS number the way Go's %g does: scientific or decimal,
// whichever is shorter (ties go to decimal). Matches the "value" strings
// in toml-test fixtures, which are emitted by BurntSushi's Go reference.
function goFloat(v: number): string {
  if (Object.is(v, -0)) return '-0'
  if (v === 0) return '0'
  const dec = '' + v
  const sci = v.toExponential().replace(
    /e([-+]?)(\d+)/,
    (_, sign, num) => 'e' + (sign || '+') + num.padStart(2, '0'),
  )
  return dec.length <= sci.length ? dec : sci
}

function firstLine(s: string) {
  const i = s.indexOf('\n')
  return 0 <= i ? s.substring(0, i) : s
}

function find(parent: string, found: any[]) {
  for (let file of Fs.readdirSync(parent)) {
    let filepath = Path.join(parent, file)
    let desc = Fs.lstatSync(filepath)
    if (desc.isDirectory()) {
      find(filepath, found)
    }
    else if (desc.isFile()) {
      let m: any = file.match(/^(.+)\.toml$/)
      if (m && m[1]) {
        found.push({
          name: Path.join(parent, m[1]),
          json: JSON.parse(
            Fs.readFileSync(Path.join(parent, m[1] + '.json')).toString()),
          toml: Fs.readFileSync(Path.join(parent, m[1] + '.toml')).toString()
        })
      }
    }
  }

  return found
}
