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

// A parse takes time in proportion to the length of the document. The Rust
// port's string matcher used to copy the whole of the rest of the source at
// every token, so its parse time grew with the SQUARE of the length (4,000
// tables took 23 seconds rather than 0.4). This runtime indexes the source
// in place; the test keeps it that way. Mirrors TestParseIsLinear in
// go/perf_test.go and parse_time_is_linear_in_document_length in
// rs/tests/perf_test.rs.
//
// Machine-independent like the test above: it compares a document with four
// times as much in it, in the same run. Linear time makes that about 4x;
// quadratic makes it 16x. The limit, 8x, sits between.
test('parse time is linear in document length', () => {
  const tables = (n: number) => {
    let src = ''
    for (let i = 0; i < n; i++) {
      src += `[[item]]\nid = ${i}\nname = "item ${i}"\ntags = ["a", "b"]\n\n`
    }
    return src
  }
  const small = 250
  const toml = new Tabnas().use(jsonic).use(Toml)
  const time = (src: string) => {
    let best = Infinity
    for (let run = 0; run < 3; run++) {
      const t0 = process.hrtime.bigint()
      toml.parse(src)
      best = Math.min(best, Number(process.hrtime.bigint() - t0))
    }
    return best
  }
  // Compile the hot paths before either measurement.
  toml.parse(tables(small))

  const few = time(tables(small))
  const many = time(tables(4 * small))
  const ratio = many / Math.max(few, 1)

  assert.ok(
    ratio <= 8,
    `parse time grows faster than document length: ${small} tables took ` +
      `${few}ns and ${4 * small} took ${many}ns (ratio ${ratio.toFixed(1)}x; ` +
      `linear is about 4x, quadratic 16x). Something per token is reading ` +
      `the rest of the source.`,
  )

  console.log(
    `perf: ${small} tables=${few}ns ${4 * small} tables=${many}ns ` +
      `ratio=${ratio.toFixed(2)}x`,
  )
})

// A dotted table header takes time in proportion to its number of segments.
// The Rust port used to walk the tree from its root on every segment, copy
// the whole path several times over and keep one more copy of it per
// segment, so its time grew faster than the square of the header's length
// (tabnas/toml#81: 10,000 segments took two minutes). This runtime hands each
// segment the table the previous one reached, as `r.prev.node`, which is
// constant work; the test keeps it that way. Mirrors TestDottedHeaderIsLinear
// in go/perf_test.go and a_dotted_header_takes_time_in_proportion_to_its_length
// in rs/tests/perf_test.rs.
//
// Machine-independent like the test above: it compares a header four times
// as long, in the same run. Linear time makes that about 4x; quadratic makes
// it 16x. The limit, 8x, sits between. A collection or a burst of load on a
// shared runner can land on either side, so each side is the fastest of five
// parses and the comparison gets three attempts.
test('a dotted header takes time in proportion to its length', () => {
  const header = (n: number) =>
    '[' + Array(n).fill('a').join('.') + ']\nx = 1\n'
  const short = 2000
  const toml = new Tabnas().use(jsonic).use(Toml)
  const time = (src: string) => {
    let best = Infinity
    for (let run = 0; run < 5; run++) {
      const t0 = process.hrtime.bigint()
      toml.parse(src)
      best = Math.min(best, Number(process.hrtime.bigint() - t0))
    }
    return best
  }
  const few = header(short)
  const many = header(4 * short)
  // Compile the hot paths before either measurement.
  toml.parse(few)
  toml.parse(many)

  const seen: string[] = []
  for (let attempt = 0; attempt < 3; attempt++) {
    const fewTime = time(few)
    const manyTime = time(many)
    const ratio = manyTime / Math.max(fewTime, 1)
    seen.push(`${fewTime}ns and ${manyTime}ns (${ratio.toFixed(1)}x)`)
    if (ratio <= 8) {
      console.log(
        `perf: ${short} segments=${fewTime}ns ${4 * short} segments=` +
          `${manyTime}ns ratio=${ratio.toFixed(2)}x`,
      )
      return
    }
  }
  assert.fail(
    `a dotted header's parse time grows faster than its length: ${short} and ` +
      `${4 * short} segments took ${seen.join(', then ')} (linear is about ` +
      `4x, quadratic 16x). Something per segment is walking the path from ` +
      `the root, copying it, or keeping a copy of it.`,
  )
})

// A header thousands of segments long builds exactly the tables it names, and
// later headers walk back down through them: one adds a table beside the first
// one's key, two arrays of tables append to the same array, and one that
// treats a key holding a value as a table is refused, with the same diagnosis
// as a short header gets. Mirrors a_long_dotted_header_builds_every_table_it_names
// in rs/tests/perf_test.rs and TestLongDottedHeaderValue in go/perf_test.go.
test('a long dotted header builds every table it names', () => {
  const depth = 5000
  const path = Array.from({ length: depth }, (_, i) => `k${i}`).join('.')
  const src =
    `[${path}]\nx = 1\n[${path}.y]\nz = 2\n` +
    `[[${path}.list]]\nn = 1\n[[${path}.list]]\nn = 2\n`
  const toml = new Tabnas().use(jsonic).use(Toml)

  // Walked with a loop: a value 5,000 deep is not compared with the call
  // stack.
  let table: any = toml.parse(src)
  for (let level = 0; level < depth; level++) {
    assert.deepStrictEqual(
      Object.keys(table),
      [`k${level}`],
      `level ${level} holds k${level} and nothing else`,
    )
    table = table[`k${level}`]
  }
  assert.strictEqual(
    JSON.stringify(table),
    '{"x":1,"y":{"z":2},"list":[{"n":1},{"n":2}]}',
  )

  assert.throws(
    () => toml.parse(`${src}[${path}.x.q]\n`),
    (error: any) =>
      'toml_key_conflict' === error.code &&
      error.message.includes('cannot define x, it already has the value 1'),
  )
})
