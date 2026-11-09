/* Copyright (c) 2026 Richard Rodger and other contributors, MIT License */

// A parse from a reader (`tn.parseReader`, admin ADR-30 and ADR-33).
//
// `make()` builds the parser whole, as `new Tabnas().use(jsonic).use(Toml)`
// builds it, and declares `lex.window` on it, so that a reader is read a
// window at a time. The plugin's own matchers, its strings, its dates and
// times and the leading byte order mark, read the source through the window
// and read on where a scan reaches its end. These tests hold it to that:
// `make()` declares the window and parses as that composition does; every
// row of the shared fixtures and every document of the toml-test corpus,
// both halves, parses the same from a reader, in pieces of 1, 3 and 64
// bytes, under windows of a few UTF-16 units and the engine's own, as from
// its text, but for the corpus documents that are not UTF-8, which a reader
// refuses as `unreadable`; and tokens longer than the window, and a long
// document, parse the same too.

import { test, describe } from 'node:test'
import assert from 'node:assert'
import { readdirSync, readFileSync, lstatSync } from 'node:fs'
import { join, relative } from 'node:path'

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { findSpecDir, loadSpec } from '@tabnas/support'

import { Toml, make } from '..'

const SPEC = findSpecDir(__dirname)
const SUITE = join(__dirname, '..', 'test', 'toml-test', 'tests')

type Reader = { read(buf: Uint8Array): number }
type Sizes = { margin: number; block: number; chunk: number }

// A reader that hands its bytes over at most `size` at a time.
function pieces(bytes: Uint8Array, size: number): Reader {
  let at = 0
  return {
    read(buf: Uint8Array): number {
      const n = Math.min(size, buf.length, bytes.length - at)
      buf.set(bytes.subarray(at, at + n))
      at += n
      return n
    },
  }
}

// A value as text that tells apart everything a parse can give: the order
// of a table's keys, a number's sign and NaN, and a date by its kind, its
// source and its time, which is NaN for a leap second (`deepStrictEqual`
// would call two such dates different).
function canon(v: any): string {
  if (v instanceof Date) {
    return 'date(' + JSON.stringify((v as any).__toml__) + ' ' + v.getTime() + ')'
  }
  if ('number' === typeof v) {
    return Object.is(v, -0) ? '-0' : String(v)
  }
  if (Array.isArray(v)) {
    return '[' + v.map(canon).join(',') + ']'
  }
  if (null != v && 'object' === typeof v) {
    return '{' + Object.keys(v).map((k) => JSON.stringify(k) + ':' + canon(v[k])).join(',') + '}'
  }
  return String(JSON.stringify(v))
}

// What a parse shows: its value, or its error's code, position, row,
// column and token text. An error's excerpt is left out: a parse from a
// reader shows a long line only as far back as the window reaches
// (tabnas/parser DIVERGENCE.md), the one difference allowed.
function seen(parse: () => unknown): string {
  try {
    return 'value ' + canon(parse())
  } catch (e: any) {
    if (null == e || 'function' !== typeof e.toJSON) {
      return 'throw ' + (e && e.name) + ' ' + (e && e.message)
    }
    const d = e.toJSON()
    return 'error ' + d.code + ' at ' + d.pos + ' ' + d.row + ':' + d.col + ' ' +
      JSON.stringify(d.token.src)
  }
}

const short = (s: string): string => (s.length > 300 ? s.slice(0, 300) + '...' : s)

// The pieces a reader hands its bytes over in, and the windows (margin and
// block) it is read under besides the engine's own: a few units, so that
// every matcher meets the window's edge.
const PIECES = [1, 3, 64]
const WINDOWS = [[1, 1], [3, 2], [64, 64]]

// `text` parsed by `tn` whole, and from a reader in every pieces size,
// under the engine's window and each of WINDOWS: how many reader parses
// were compared. A difference is pushed onto `failures`.
function sameFromReader(tn: Tabnas, at: string, text: string, failures: string[]): number {
  const whole = seen(() => tn.parse(text))
  const bytes = Buffer.from(text, 'utf8')
  const anyTn = tn as any
  let compared = 0
  for (const size of PIECES) {
    const reads: [string, () => unknown][] = [['', () => anyTn.parseReader(pieces(bytes, size))]]
    for (const [margin, block] of WINDOWS) {
      const sizes: Sizes = { margin, block, chunk: size }
      reads.push([
        ', margin ' + margin + ', block ' + block,
        () => anyTn.parseReaderWithSizes(pieces(bytes, size), sizes),
      ])
    }
    for (const [how, parse] of reads) {
      const read = seen(parse)
      compared++
      if (read !== whole) {
        failures.push(
          at + ': ' + JSON.stringify(text.slice(0, 80)) + ' from a reader in pieces of ' +
          size + how + '\n  whole:  ' + short(whole) + '\n  reader: ' + short(read),
        )
      }
    }
  }
  return compared
}

// Every `.toml` document under `dir`, by its path from `base`.
function documents(base: string, dir: string = base, found: string[] = []): string[] {
  for (const file of readdirSync(dir).sort()) {
    const path = join(dir, file)
    if (lstatSync(path).isDirectory()) {
      documents(base, path, found)
    } else if (file.endsWith('.toml')) {
      found.push(relative(base, path))
    }
  }
  return found
}

describe('window', () => {
  test('make-declares-the-window', () => {
    const tn = make() as any
    assert.equal(tn.internal().config.lex.window, true)
    // The plugin alone declares nothing: a host may install it beneath
    // matchers of its own.
    const composed = new Tabnas().use(jsonic).use(Toml) as any
    assert.ok(!composed.internal().config.lex.window)
  })

  test('make-parses-as-the-composition-does', () => {
    // Every fixture row, whole, through `make()` and through
    // `new Tabnas().use(jsonic).use(Toml)`, which dmog composed.
    let compared = 0
    const failures: string[] = []
    for (const file of readdirSync(SPEC).filter((f) => f.endsWith('.tsv')).sort()) {
      for (const row of loadSpec(join(SPEC, file)).rows) {
        const input = row.unescNamed('input')
        const made = seen(() => make().parse(input))
        const composed = seen(() => new Tabnas().use(jsonic).use(Toml).parse(input))
        compared++
        if (made !== composed) {
          failures.push(file + ':' + row.line + '\n  make:     ' + made + '\n  composed: ' + composed)
        }
      }
    }
    assert.ok(200 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('every-row-parses-the-same-from-a-reader', () => {
    const files = readdirSync(SPEC).filter((f) => f.endsWith('.tsv')).sort()
    let compared = 0
    const failures: string[] = []
    files.forEach((file, index) => {
      for (const row of loadSpec(join(SPEC, file)).rows) {
        compared += sameFromReader(make(), file + ':' + row.line, row.unescNamed('input'), failures)
      }
      console.log('window fixtures: ' + (index + 1) + ' of ' + files.length + ' files (' +
        Math.floor(((index + 1) * 100) / files.length) + '%), ' + compared + ' reader parses')
    })
    assert.ok(2400 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('the-corpus-parses-the-same-from-a-reader', () => {
    // Both halves of BurntSushi/toml-test, as toml.test.ts reads them
    // (`npm test` fetches the corpus first, and that suite fails when it
    // is missing).
    const tn = make()
    const decoder = new TextDecoder('utf-8', { fatal: true })
    const halves = ['valid', 'invalid']
    let compared = 0
    let refused = 0
    const failures: string[] = []
    halves.forEach((half, index) => {
      const base = join(SUITE, half)
      for (const name of documents(base)) {
        const bytes = readFileSync(join(base, name))
        let text: string
        try {
          text = decoder.decode(bytes)
        } catch (e) {
          // Not UTF-8: a reader refuses it, where a parse of a decoded
          // string has no such bytes to refuse.
          const read = seen(() => (tn as any).parseReader(pieces(bytes, 3)))
          refused++
          if (!read.startsWith('error unreadable ')) {
            failures.push(half + '/' + name + ': from a reader ' + read + ', not unreadable')
          }
          continue
        }
        compared += sameFromReader(tn, half + '/' + name, text, failures)
      }
      console.log('window corpus: ' + (index + 1) + ' of ' + halves.length + ' halves (' +
        Math.floor(((index + 1) * 100) / halves.length) + '%), ' + compared + ' reader parses')
    })
    // The nine documents of invalid/encoding that are not UTF-8.
    assert.equal(refused, 9)
    assert.ok(9000 < compared, '' + compared)
    assert.deepEqual(failures, [])
  })

  test('tokens-longer-than-the-window-parse-the-same-from-a-reader', () => {
    // The engine's window reaches 64 KiB past the cursor before each
    // token: a token longer than that meets its edge under the engine's
    // own sizes. Each kind of token the plugin cuts itself, and the
    // engine's around them, long, and cut short by the source's end.
    const long = (64 << 10) + 2048
    const docs = [
      'a = "' + 's'.repeat(long) + '\\u00e9\\U0001F600\\x41"\nb = 1',
      'a = "' + 'é'.repeat(long) + '"',
      "a = '" + 'l'.repeat(long) + "'",
      'a = """\n' + 'm\\n"x'.repeat(long / 5) + '"""""',
      'a = """' + 'w'.repeat(long) + '\\\n' + ' \n\t'.repeat(long / 3) + 'z"""',
      "a = '''" + "q'\n".repeat(long / 3) + "''''",
      'a = 1979-05-27T07:32:00.' + '9'.repeat(long) + '+07:00\nb = 2',
      'a = 07:32:00.' + '5'.repeat(long) + '\nb = 2',
      'a = 1979-05-27 07:32:00.' + '1'.repeat(long) + 'Z',
      'k' + 'e'.repeat(long) + 'y = 1\n"' + 'q'.repeat(long) + '" = 2',
      '# ' + 'c'.repeat(long) + '\na = 1',
      'a = 1' + ' '.repeat(long) + '\n' + '\n'.repeat(long) + 'b = 2',
      'a = 1' + '0'.repeat(long),
      'a = 1.' + '5'.repeat(long) + 'e3',
      'a = [' + '1, '.repeat(long / 3) + '2]',
      // Cut short, or refused, past the edge.
      'a = "' + 'u'.repeat(long),
      'a = """' + 'u\n'.repeat(long / 2),
      'a = "' + 'p'.repeat(long) + '\u0001"',
      'a = "' + 'x'.repeat(long) + '\\x4"',
      'a = "' + 'x'.repeat(long) + '\\u00',
      'a = 1979-05-27T07:32:00.' + '9'.repeat(long) + '+07:',
      'a = 2006-01-32T07:32:00.' + '9'.repeat(long),
    ]
    const tn = make() as any
    const failures: string[] = []
    docs.forEach((doc, n) => {
      const whole = seen(() => tn.parse(doc))
      for (const size of [4096, (64 << 10) + 3]) {
        const read = seen(() => tn.parseReader(pieces(Buffer.from(doc, 'utf8'), size)))
        if (read !== whole) {
          failures.push('document ' + n + ' in pieces of ' + size + ': whole ' +
            short(whole) + ', reader ' + short(read))
        }
      }
      console.log('window long tokens: ' + (n + 1) + ' of ' + docs.length + ' (' +
        Math.floor(((n + 1) * 100) / docs.length) + '%)')
    })
    assert.deepEqual(failures, [])
  })

  test('a-long-document-parses-the-same-from-a-reader', () => {
    // From a reader in pieces of 4 KiB, under the engine's window and
    // under one of 256 units, which moves on thousands of times; with an
    // error at the end too, which the reader places where the whole text
    // does. A few hundred KB: the engine keeps every rule a parse makes,
    // which grows its heap with the document.
    const parts: string[] = ['# records\ntitle = "records"\n']
    for (let i = 0; i < 2500; i++) {
      parts.push(
        '\n[[record]]\nid = ' + i + '\nname = "ñ' + i + '\\u00e9" # note ' + i +
        "\npath = 'C:\\dir\\" + i + "'\nwhen = 1979-05-27T07:32:" + String(i % 60).padStart(2, '0') +
        '.' + i + '-07:00\nat = 07:32:00.5\nday = 2026-11-09\nscore = -1.5e3\n' +
        'tags = ["a", "b", { k = ' + i + ' }]\nnote = """\nline ' + i + '\\\n  joined"""\n' +
        'deep.er.key = true\n',
      )
    }
    const body = parts.join('')
    const text = body + '\n[end]\nok = true\n'
    assert.ok(300 << 10 < text.length, '' + text.length)
    const broken = body + '\n[end]\nok = "tail\u0001"\n'
    const tn = make() as any
    ;[text, broken].forEach((doc, n) => {
      const whole = seen(() => tn.parse(doc))
      assert.equal(whole.startsWith('value '), 0 === n, short(whole))
      for (const sizes of [undefined, { margin: 256, block: 256, chunk: 4096 }]) {
        const bytes = Buffer.from(doc, 'utf8')
        const read = seen(() =>
          undefined === sizes
            ? tn.parseReader(pieces(bytes, 4096))
            : tn.parseReaderWithSizes(pieces(bytes, 4096), sizes),
        )
        assert.equal(read, whole)
      }
      console.log('window long document: ' + (n + 1) + ' of 2 (' + (n + 1) * 50 + '%)')
    })
  })
})
