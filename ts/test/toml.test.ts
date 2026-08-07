/* Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License */

// BurntSushi/toml-test conformance harness.
//
// This suite runs the authoritative third-party TOML conformance corpus:
//
//   upstream: https://github.com/BurntSushi/toml-test
//   pinned:   9eef1b959e0449d41a31d4e4e0a839faee534b36
//
// The corpus is NOT committed (project rule: no vendored third-party
// test corpora). It is fetched by `scripts/fetch-toml-test.sh` into the
// gitignored `ts/test/toml-test/`. `npm test` runs that script via the
// `pretest` hook; this file also fetches on demand as a belt-and-braces
// fallback.
//
// THIS SUITE MUST NEVER SKIP. A conformance test that quietly does not
// run is worse than no test at all, because the green tick is a lie.
// If the corpus cannot be obtained the tests FAIL LOUDLY.
//
// It asserts BOTH halves:
//   valid/   — must parse AND produce the correct VALUE
//   invalid/ — must be REJECTED with an error
//
// WHICH TOML VERSION IS JUDGED. `README.md` claims "A TOML parser" and
// links to https://toml.io — whose released specification is v1.0.0
// (v1.1.0 is still unreleased). So the ASSERTED corpus is the suite's
// own `tests/files-toml-1.0.0` manifest. The v1.1.0-only delta is still
// run and REPORTED (see the `toml-1.1.0-report` test) so nothing is
// concealed; it is simply not asserted against, because the package
// does not claim v1.1.0.
//
// NO NAME-KEYED FIXUPS. Earlier revisions of this harness rewrote the
// parsed value based on the FIXTURE NAME (an `allFloat` allow-list, a
// saturating int64 hack for `integer/long`, a `3.0e14` special case for
// `integer/underscore`, a `ten = 1e3` patch, a `-0` rescue for
// `float/zero`). Those made failing fixtures pass, which is the exact
// defect this suite exists to expose. They are gone. The only number
// rule now is the one an ordinary consumer of this API would have to
// use: integer-looking -> integer, otherwise float. Fixtures that fail
// because the parser returns an untyped JS number and the int/float
// distinction is unrecoverable are REAL conformance failures and are
// reported as such.

import { test, describe } from 'node:test'
import Fs from 'node:fs'
import Path from 'node:path'
import { execFileSync } from 'node:child_process'
import { deepStrictEqual as equal, ok } from 'node:assert/strict'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Toml } from '..'


const SUITE_URL = 'https://github.com/BurntSushi/toml-test'
const SUITE_PIN = '9eef1b959e0449d41a31d4e4e0a839faee534b36'

const REPO_ROOT = Path.join(__dirname, '..', '..')
const SUITE_ROOT = Path.join(REPO_ROOT, 'ts', 'test', 'toml-test')
const FETCH_SCRIPT = Path.join(REPO_ROOT, 'scripts', 'fetch-toml-test.sh')


// Guarantee the corpus is on disk. Never skips: if it cannot be
// obtained, throw with an actionable message so the suite goes red.
function ensureCorpus(): string {
  const valid = Path.join(SUITE_ROOT, 'tests', 'valid')
  const invalid = Path.join(SUITE_ROOT, 'tests', 'invalid')

  if (!Fs.existsSync(valid) || !Fs.existsSync(invalid)) {
    try {
      execFileSync('bash', [FETCH_SCRIPT], { stdio: 'inherit' })
    }
    catch (e: any) {
      throw new Error(
        `BurntSushi/toml-test conformance corpus is MISSING and could not be fetched.\n` +
        `  suite:  ${SUITE_URL} @ ${SUITE_PIN}\n` +
        `  expect: ${SUITE_ROOT}\n` +
        `  fix:    bash ${FETCH_SCRIPT}\n` +
        `This test deliberately FAILS rather than skipping — a conformance\n` +
        `test that silently does not run reports a green tick that is a lie.\n` +
        `  cause:  ${e && e.message}`
      )
    }
  }

  if (!Fs.existsSync(valid) || !Fs.existsSync(invalid)) {
    throw new Error(
      `BurntSushi/toml-test conformance corpus still absent after running\n` +
      `  bash ${FETCH_SCRIPT}\n` +
      `Expected ${valid} and ${invalid}.`
    )
  }

  return SUITE_ROOT
}


// The suite ships per-version manifests listing exactly which fixture
// files belong to which TOML version. Use them rather than inventing a
// selection of our own.
function versionManifest(root: string, version: string): Set<string> {
  const file = Path.join(root, 'tests', 'files-toml-' + version)
  if (!Fs.existsSync(file)) {
    throw new Error(`toml-test version manifest missing: ${file}`)
  }
  return new Set(
    Fs.readFileSync(file, 'utf8')
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter((s) => s.endsWith('.toml'))
  )
}


type Fixture = { rel: string; toml: string; json?: any }


function collect(root: string, half: 'valid' | 'invalid'): Fixture[] {
  const base = Path.join(root, 'tests', half)
  const out: Fixture[] = []

  const walk = (dir: string) => {
    for (const entry of Fs.readdirSync(dir).sort()) {
      const full = Path.join(dir, entry)
      const st = Fs.lstatSync(full)
      if (st.isDirectory()) walk(full)
      else if (st.isFile() && entry.endsWith('.toml')) {
        const rel = half + '/' + Path.relative(base, full).split(Path.sep).join('/')
        const f: Fixture = { rel, toml: Fs.readFileSync(full, 'utf8') }
        if ('valid' === half) {
          const jsonPath = full.replace(/\.toml$/, '.json')
          f.json = JSON.parse(Fs.readFileSync(jsonPath, 'utf8'))
        }
        out.push(f)
      }
    }
  }

  walk(base)
  return out
}


// --- value normalisation -------------------------------------------------
//
// toml-test fixtures encode every scalar as {type, value} with a string
// value. Convert a parse result into that shape. Deliberately
// NAME-BLIND: the fixture name is never consulted.

function tomlType(kind: string): string {
  return ({
    'offset-date-time': 'datetime',
    'local-date-time': 'datetime-local',
    'local-date': 'date-local',
    'local-time': 'time-local',
  } as any)[kind] || kind
}


// Canonicalise a TOML datetime string. Applied to BOTH the parsed value
// and the expected fixture value, so it can only collapse
// representations that are semantically identical (case of the T/Z
// separators, a space used as the date/time separator, trailing zeros in
// the fractional second, an omitted `:SS`). It cannot mask a wrong
// value.
function canonDatetime(s: string): string {
  let v = s.trim()
  v = v.replace(/^(\d{4}-\d\d-\d\d)[ tT]/, '$1T')
  v = v.replace(/[zZ]$/, 'Z')
  // Pad an omitted seconds field: HH:MM -> HH:MM:00
  v = v.replace(/(T|^)(\d\d:\d\d)(?=$|[.Z+-])/, '$1$2:00')
  // Trim trailing zeros in the fractional second; drop an empty fraction.
  v = v.replace(/\.(\d*?)0+(?=$|[Z+-])/, (_m, d) => (d ? '.' + d : ''))
  return v
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


function normValue(v: any): any {
  if (null === v || undefined === v) return v

  if (Array.isArray(v)) return v.map(normValue)

  const t = typeof v

  if ('number' === t) {
    if (Number.isNaN(v)) return { type: 'float', value: 'nan' }
    if (Infinity === v) return { type: 'float', value: 'inf' }
    if (-Infinity === v) return { type: 'float', value: '-inf' }
    // The parser hands back an untyped JS number: the only rule
    // available to a consumer is "integer-looking means integer".
    const s = '' + v
    if (s.match(/^-?[0-9]+$/)) {
      return { type: 'integer', value: s }
    }
    return { type: 'float', value: goFloat(v) }
  }

  if ('boolean' === t) return { type: 'bool', value: '' + v }

  if ('string' === t) return { type: 'string', value: v }

  if ('object' === t) {
    // TOML date/time values arrive as JS Date carrying __toml__ metadata.
    const meta = (v as any).__toml__
    if (meta) {
      return { type: tomlType(meta.kind), value: canonDatetime('' + meta.src) }
    }
    const out: any = {}
    for (const k of Object.keys(v)) out[k] = normValue((v as any)[k])
    return out
  }

  return v
}


// Canonicalise the EXPECTED fixture tree the same way for datetimes so
// the comparison is symmetric.
function canonExpected(v: any): any {
  if (Array.isArray(v)) return v.map(canonExpected)
  if (null != v && 'object' === typeof v) {
    if ('string' === typeof v.type && 'string' === typeof v.value) {
      if (v.type.startsWith('datetime') || v.type.startsWith('date-') ||
        v.type.startsWith('time-')) {
        return { type: v.type, value: canonDatetime(v.value) }
      }
      return v
    }
    const out: any = {}
    for (const k of Object.keys(v)) out[k] = canonExpected(v[k])
    return out
  }
  return v
}


function makeToml() {
  // The documented setup from README.md.
  return new Tabnas().use(jsonic).use(Toml)
}


type Report = {
  total: number
  pass: number
  fail: number
  fails: string[]
  // Invalid documents that were rejected by an internal runtime crash
  // (TypeError/RangeError with no parser error `code`) rather than by a
  // diagnosed parse error. They still count as rejected — an error was
  // thrown — but they are reported separately because a crash is not a
  // conformant rejection and the distinction must not be lost.
  crashRejects: string[]
}


function newReport(total: number): Report {
  return { total, pass: 0, fail: 0, fails: [], crashRejects: [] }
}


// Tabnas/jsonic parse errors carry a `.code`. Anything without one that
// is a plain JS runtime error is an internal crash.
function isInternalCrash(e: any): boolean {
  return !(e && 'string' === typeof e.code)
}


function runValid(fixtures: Fixture[]): Report {
  const toml = makeToml()
  const r = newReport(fixtures.length)

  for (const f of fixtures) {
    let got: any
    try {
      got = normValue(toml.parse(f.toml))
    }
    catch (e: any) {
      r.fail++
      r.fails.push(`${f.rel}  PARSE ERROR: ${('' + (e && e.message)).split('\n')[0]}`)
      continue
    }
    try {
      equal(
        JSON.parse(JSON.stringify(got)),
        JSON.parse(JSON.stringify(canonExpected(f.json))),
      )
      r.pass++
    }
    catch (e: any) {
      r.fail++
      r.fails.push(
        `${f.rel}  WRONG VALUE\n` +
        `      got:  ${JSON.stringify(got)}\n` +
        `      want: ${JSON.stringify(canonExpected(f.json))}`
      )
    }
  }

  return r
}


function runInvalid(fixtures: Fixture[]): Report {
  const toml = makeToml()
  const r = newReport(fixtures.length)

  for (const f of fixtures) {
    let out: any
    try {
      out = toml.parse(f.toml)
    }
    catch (e: any) {
      // Rejected. Record HOW it was rejected.
      r.pass++
      if (isInternalCrash(e)) {
        r.crashRejects.push(
          `${f.rel}: ${e && e.constructor && e.constructor.name}: ` +
          `${('' + (e && e.message)).split('\n')[0]}`)
      }
      continue
    }
    r.fail++
    r.fails.push(
      `${f.rel}  WRONGLY ACCEPTED\n` +
      `      src:  ${JSON.stringify(f.toml)}\n` +
      `      got:  ${JSON.stringify(out)}`
    )
  }

  return r
}


function summary(label: string, r: Report): string {
  const pct = r.total ? ((100 * r.pass) / r.total).toFixed(1) : '0.0'
  const crash = r.crashRejects.length
    ? `  (of which ${r.crashRejects.length} rejected by INTERNAL CRASH, not a diagnosed parse error)`
    : ''
  return `${label}: ${r.pass}/${r.total} (${pct}%)  failures=${r.fail}${crash}`
}


// How many individual failures to print. The assertion message always
// carries the full list; this only bounds the console noise. Raise with
// TOML_CONFORMANCE_MAX_FAIL=0 (unlimited) when triaging.
const MAX_FAIL = (() => {
  const raw = process.env.TOML_CONFORMANCE_MAX_FAIL
  if (null == raw || '' === raw) return 40
  const n = parseInt(raw, 10)
  return Number.isFinite(n) && n >= 0 ? (0 === n ? Infinity : n) : 40
})()


function reportFailures(label: string, r: Report, limit = MAX_FAIL) {
  console.log(summary(label, r))
  for (const line of r.fails.slice(0, limit)) console.log('  FAIL ' + line)
  if (r.fails.length > limit) {
    console.log(`  ...and ${r.fails.length - limit} more failures`)
  }
  for (const line of r.crashRejects.slice(0, limit)) {
    console.log('  CRASH-REJECT ' + line)
  }
  if (r.crashRejects.length > limit) {
    console.log(`  ...and ${r.crashRejects.length - limit} more crash-rejections`)
  }
}


describe('toml-conformance', () => {

  test('toml-valid', () => {
    const root = ensureCorpus()
    const v100 = versionManifest(root, '1.0.0')
    const all = collect(root, 'valid')
    const claimed = all.filter((f) => v100.has(f.rel))

    ok(claimed.length > 0,
      'toml-test valid/ corpus for TOML 1.0.0 is empty — corpus is broken')

    const r = runValid(claimed)
    reportFailures('toml-valid (TOML 1.0.0)', r)

    equal(r.fail, 0,
      `${r.fail} of ${r.total} TOML 1.0.0 valid documents did not parse to the ` +
      `expected value (suite ${SUITE_URL} @ ${SUITE_PIN}):\n` +
      r.fails.map((s) => '  ' + s).join('\n'))
  })


  test('toml-invalid', () => {
    const root = ensureCorpus()
    const v100 = versionManifest(root, '1.0.0')
    const all = collect(root, 'invalid')
    const claimed = all.filter((f) => v100.has(f.rel))

    ok(claimed.length > 0,
      'toml-test invalid/ corpus for TOML 1.0.0 is empty — corpus is broken')

    const r = runInvalid(claimed)
    reportFailures('toml-invalid (TOML 1.0.0)', r)

    equal(r.fail, 0,
      `${r.fail} of ${r.total} TOML 1.0.0 INVALID documents were wrongly ` +
      `ACCEPTED (suite ${SUITE_URL} @ ${SUITE_PIN}):\n` +
      r.fails.map((s) => '  ' + s).join('\n'))
  })


  // Informational only. The package claims TOML (i.e. the released
  // v1.0.0 spec), not the unreleased v1.1.0 draft, so this is measured
  // and printed but not asserted. It exists so the full corpus is never
  // hidden.
  test('toml-1.1.0-report', () => {
    const root = ensureCorpus()
    const v110 = versionManifest(root, '1.1.0')

    const valid = collect(root, 'valid').filter((f) => v110.has(f.rel))
    const invalid = collect(root, 'invalid').filter((f) => v110.has(f.rel))

    const rv = runValid(valid)
    const ri = runInvalid(invalid)

    console.log('--- TOML 1.1.0 draft (reported, NOT asserted: not claimed) ---')
    console.log('  ' + summary('toml-1.1.0-valid', rv))
    console.log('  ' + summary('toml-1.1.0-invalid', ri))

    const all = collect(root, 'valid')
    const allInvalid = collect(root, 'invalid')
    console.log('--- Whole corpus (reported, NOT asserted) ---')
    console.log('  ' + summary('all-valid', runValid(all)))
    console.log('  ' + summary('all-invalid', runInvalid(allInvalid)))
  })

})
