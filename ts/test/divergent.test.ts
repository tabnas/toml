/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// The divergence register: where this repo's two ports DISAGREE, executed.
//
// `go/divergent_test.go` runs the SAME file and reads the other column.
//
// WHY THIS IS NOT A FIXTURE. A fixture fails when behaviour REGRESSES. This
// fails BOTH ways: when a port is repaired to agree with the other, the row
// still claims they differ, so the suite goes red and names the row to
// delete. A divergence recorded as a passing test of current behaviour
// survives its own repair — the port is fixed, the test is updated, and the
// record now describes something that no longer happens, with nothing red.
// That is how the 2026-08 fleet audit found 29 recorded claims contradicted
// by execution.
//
// The runner is LOCAL for now. `@tabnas/support` gains this mechanism in
// tabnas/support#14, and the row vocabulary here is deliberately the one
// that PR standardises — including `@<row>:<col>` from support#12 — so
// adopting it deletes this file and leaves the fixture untouched.

import { test, describe } from 'node:test'
import { ok, equal } from 'node:assert/strict'
import Path from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import {
  findSpecDir, loadSpec, isErrorExpect, parseExpect, equalValue,
} from '@tabnas/support'

import { Toml } from '..'


const REGISTER = Path.join(findSpecDir(__dirname), '..', 'divergent.tsv')

// This runtime's column. The Go half reads `go`.
const RUNTIME = 'ts'
const OTHER = 'go'


// What one port did with one input, in the register's own vocabulary.
function outcome(src: string): string {
  const tn = new Tabnas().use(jsonic).use(Toml)
  try {
    return JSON.stringify(tn.parse(src))
  }
  catch (err: any) {
    // These errors carry the position as lineNumber/columnNumber, not
    // row/col. Worth knowing before adopting support#12, whose default
    // reader looks for row/col and would find neither.
    const at = null == err.lineNumber
      ? '' : `@${err.lineNumber}:${err.columnNumber}`
    return `ERROR:${err.code ?? '?'}${at}`
  }
}


// `ERROR:unexpected@1:8` -> ['unexpected', '1:8']; `ERROR:unexpected` ->
// ['unexpected', ''].
//
// PARSED HERE, NOT BY THE SUPPORT LIBRARY, and that is deliberate. This
// comparator used to ask `errorCode` for the code and then look for an
// `@1:8` suffix on the answer. tabnas/support#12 split the position OUT of
// that value, so the suffix was no longer there to find: every cell parsed
// as "pins no position", every pair of rows sharing a code compared EQUAL,
// and this whole register silently became rows that assert nothing — the
// exact failure it exists to catch, arriving through a dependency rather
// than an edit.
//
// The obvious repair — call the newer `errorExpect` and read row/col off
// the result — swaps one coupling for another AND throws at runtime
// against the PUBLISHED `@tabnas/support`, which does not export it
// (0.3.2 has `errorCode` only, and its `errorCode` still returns the
// position attached). Green CI hid it: the shared polyglot-ci workflow
// symlinks the SIBLING checkout, where the function exists.
//
// The cell format is this repo's own contract, documented in
// test/AGENTS.md, so this repo reads it. No support version can change
// what it means. go/divergent_test.go is repaired the same way.
function splitCell(cell: string): [string, string] {
  const code = cell.startsWith('ERROR:') ? cell.slice('ERROR:'.length) : cell
  const at = code.match(/@(\d+:\d+)$/)
  return at ? [code.slice(0, at.index), at[1]] : [code, '']
}

// Do two cells MEAN the same thing? Compared by meaning, not bytes: `1` and
// `1.0` are one expectation, and a row whose columns differ only that way
// records no divergence at all.
function same(a: string, b: string): boolean {
  if (a === b) return true

  if (isErrorExpect(a) || isErrorExpect(b)) {
    if (!isErrorExpect(a) || !isErrorExpect(b)) return false

    const [ca, pa] = splitCell(a)
    const [cb, pb] = splitCell(b)
    if (ca !== cb) return false

    // POSITION IS OPT-IN. A cell that pins no position is satisfied by any
    // position, because most rows here are about the code and pinning the
    // column of every one of them would make the register fail on changes
    // it is not recording. A cell that DOES pin one is compared on both.
    // Same rule as tabnas/support#12.
    return '' === pa || '' === pb || pa === pb
  }

  try {
    return equalValue(parseExpect(a), parseExpect(b))
  }
  catch {
    return false
  }
}


describe('divergence-register', () => {
  const spec = loadSpec(REGISTER)

  ok(0 < spec.rows.length,
    'divergent.tsv has no rows. An EMPTY register is legitimate — a repo ' +
    'with no divergences — but an empty FILE is not: it cannot be told ' +
    'apart from a loader that read nothing.')

  for (const row of spec.rows) {
    const input = row.unesc(row.resolve('input'))
    const mine = row.col(row.resolve(RUNTIME))
    const theirs = row.col(row.resolve(OTHER))

    test(`row ${row.line}: ${JSON.stringify(input)}`, () => {
      // 1. Does this row record a divergence at all? Two columns saying
      //    the same thing assert nothing and would pass forever, which is
      //    the shape of the prose claims this replaces.
      ok(!same(mine, theirs),
        `${row.where()}: both columns mean ${JSON.stringify(mine)}, so this ` +
        'row records no divergence and can never fail meaningfully. Delete ' +
        'it, or correct the cells to what the ports actually do.')

      const got = outcome(input)

      if (same(got, mine)) {
        return
      }

      // 2. It changed. Did it change INTO the other port's answer? Then
      //    the divergence is closed, and reporting a regression would send
      //    the reader to exactly the wrong conclusion.
      if (same(got, theirs)) {
        ok(false,
          `${row.where()}: this divergence is CLOSED. ${RUNTIME} now ` +
          `produces what the ${OTHER} column records (${theirs}), not its ` +
          `own (${mine}).\n  A fixed divergence fails as loudly as a ` +
          'regressed one, so the row cannot outlive it.\n  DELETE this row ' +
          '— and if the repair landed in the engine, check whether the ' +
          `other rows citing ${row.col(row.resolve('why'))} go with it.`)
      }

      // 3. Neither. An ordinary regression.
      equal(got, mine,
        `${row.where()}: ${RUNTIME} changed, and not into the ${OTHER} ` +
        'answer either — this is a regression, not a closed divergence.')
    })
  }
})


// Pins the comparator itself, because a comparator that stops
// distinguishing positions does not fail — it makes every position row in
// the register read as "records no divergence", and the register becomes a
// file of rows that assert nothing while staying green on the rows that
// survive.
//
// That is not hypothetical. `same()` used to re-parse an `@1:8` suffix off
// `errorCode`, and tabnas/support#12 split the position OUT of that value —
// so every cell parsed as "pins no position" and every pair sharing a code
// compared equal. Nothing in this repo asserted the comparator's own
// behaviour, so the only symptom was the vacuity check firing on rows that
// were, in fact, perfectly good.
//
// go/divergent_test.go asserts the same four cases.
describe('divergence-register comparator', () => {
  test('reads the position, and treats an unpinned one as opt-in', () => {
    const cases: [string, string, string, boolean][] = [
      // The case that regressed: same code, different column.
      ['differing column', 'ERROR:unexpected@1:5', 'ERROR:unexpected@1:6', false],
      ['differing row', 'ERROR:unexpected@1:5', 'ERROR:unexpected@2:5', false],

      // Controls. Without these, "distinguishes positions" is also
      // satisfied by a comparator that calls everything different.
      ['identical position', 'ERROR:unexpected@1:5', 'ERROR:unexpected@1:5', true],
      ['position is opt-in', 'ERROR:unexpected', 'ERROR:unexpected@1:5', true],
    ]
    for (const [name, a, b, want] of cases) {
      equal(same(a, b), want, `${name}: same(${a}, ${b})`)
    }
  })
})
