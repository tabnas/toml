/* Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License */

import { test, describe } from 'node:test'
import Fs from 'node:fs'
import Path from 'node:path'
import { execFileSync } from 'node:child_process'
import { deepStrictEqual as equal, ok } from 'node:assert/strict'

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
//   invalid/  must be rejected. Asserted against a measured FLOOR (see
//             INVALID_FLOOR below), because the parser does not reject
//             them all yet and a floor is what ratchets.

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
    ok(found.length >= INVALID_TOTAL,
      `toml-test invalid suite looks truncated: ${found.length} fixtures ` +
      `found, expected at least ${INVALID_TOTAL}`)

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

    ok(rejected >= INVALID_FLOOR,
      `BurntSushi/toml-test invalid suite REGRESSED: ${rejected} of ` +
      `${found.length} rejected, floor is ${INVALID_FLOOR} ` +
      `(suite ${SUITE_URL} @ ${SUITE_PIN}). Documents that must be ` +
      'rejected are now being accepted. Raise the floor when the grammar ' +
      'improves; never lower it to make this pass.')

    ok(diagnosed >= INVALID_DIAGNOSED_FLOOR,
      `BurntSushi/toml-test invalid suite REGRESSED: only ${diagnosed} of ` +
      `${found.length} rejections are diagnosed parse errors, floor is ` +
      `${INVALID_DIAGNOSED_FLOOR}. A rejection that is really an internal ` +
      'crash does not count as conformance.')
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
  // It is what raises INVALID_DIAGNOSED_FLOOR, 212 -> 245, above.
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
})


// Floors for the invalid half, MEASURED on 2026-08-09 against
// BurntSushi/toml-test @ 9eef1b9 with @tabnas/toml at 0.5.0. These are a
// ratchet, not a target: raise them when the grammar tightens, never
// lower them. See test('toml-invalid').
//
// RAISED 2026-08-19, same corpus pin, after key-conflict detection replaced
// six uncaught TypeErrors with a `toml_key_conflict` diagnosis. Measured
// 245 rejected / 245 diagnosed / 0 internal crashes, from 242 / 227 / 15.
// The diagnosed floor moves furthest because that is what the change does:
// the crashes were already counted as rejections, just not conformant ones.
const INVALID_TOTAL = 509
const INVALID_FLOOR = 245
const INVALID_DIAGNOSED_FLOOR = 245

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
