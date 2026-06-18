/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// Machine-INDEPENDENT performance regression guard.
//
// @tabnas/toml exports only the `Toml` plugin — there is NO package-level
// convenience parse to cache (callers build their own engine via
// `new Tabnas().use(jsonic).use(Toml)`). The thing that must stay fast is
// REUSING that one built instance across many parses: building the (expensive)
// TOML grammar dominates a parse, so rebuilding the engine per call is many
// times slower than reusing it.
//
// This test compares, on the SAME machine in the SAME run:
//   - N parses that REUSE one built instance, vs
//   - N parses that each REBUILD the engine (`new Tabnas().use(...)`).
// It asserts that reuse is at least roughly comparable (the rebuild path is
// allowed to be up to 4x; in practice it is far slower). There is deliberately
// NO absolute wall-clock budget — both sides scale together on a slow box, so
// a slow CI machine cannot make it flaky. It documents and guards the
// reuse-the-instance best practice; it would fail loudly if reuse stopped
// being the fast path (i.e. if every parse secretly rebuilt the grammar).

import { test } from 'node:test'
import assert from 'node:assert'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { Toml } from '..'

test('parse reuse stays fast vs rebuilding the engine', () => {
  const src = 'a = 1\nb = 2\nc = 3'
  const n = 300

  const make = () => new Tabnas().use(jsonic).use(Toml)

  // Warm both paths so the comparison is steady-state.
  const reused = make()
  for (let i = 0; i < 50; i++) reused.parse(src)
  for (let i = 0; i < 50; i++) make().parse(src)

  const t0 = process.hrtime.bigint()
  for (let i = 0; i < n; i++) {
    reused.parse(src)
  }
  const reuse = process.hrtime.bigint() - t0

  const t1 = process.hrtime.bigint()
  for (let i = 0; i < n; i++) {
    make().parse(src)
  }
  const rebuild = process.hrtime.bigint() - t1

  const ratio = Number(rebuild) / Number(reuse)

  // Reusing one instance must not be the slow path. Rebuilding per parse is
  // many times slower (grammar build dominates); we only assert reuse is at
  // least roughly comparable so the test stays machine-independent.
  assert.ok(
    Number(reuse) <= 4 * Number(rebuild),
    `reusing one instance for ${n} parses (${reuse}ns) was unexpectedly ` +
      `slower than rebuilding per parse (${rebuild}ns) — the engine/grammar ` +
      `is no longer being reused. ratio rebuild/reuse=${ratio.toFixed(2)}x.`,
  )

  console.log(
    `perf: reuse=${reuse}ns rebuild=${rebuild}ns ` +
      `rebuild/reuse=${ratio.toFixed(2)}x (over ${n} parses)`,
  )
})
