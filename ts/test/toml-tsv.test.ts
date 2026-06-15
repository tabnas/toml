/* Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License */

import { test, describe } from 'node:test'
import { deepStrictEqual, throws } from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { Jsonic } from '@tabnas/jsonic'
import { Toml } from '..'


function unescape(str: string): string {
  return str.replace(/\\r\\n|\\n|\\r|\\t/g, (m) => {
    if (m === '\\r\\n') return '\r\n'
    if (m === '\\n') return '\n'
    if (m === '\\r') return '\r'
    if (m === '\\t') return '\t'
    return m
  })
}


function loadTSV(name: string): { input: string; expected: string; row: number }[] {
  const specPath = join(__dirname, '..', '..', 'test', 'spec', name + '.tsv')
  const lines = readFileSync(specPath, 'utf8').split(/\r?\n/).filter(Boolean)
  return lines.slice(1).map((line, i) => {
    const cols = line.split('\t').map(unescape)
    return { input: cols[0], expected: cols[1], row: i + 2 }
  })
}


function makeToml() {
  return Jsonic.make().use(Toml)
}


// Normalize actual output so null-prototype maps (produced by Jsonic) compare
// equal to plain JSON objects via deepStrictEqual.
function normalize(v: any): any {
  return JSON.parse(JSON.stringify(v))
}


function runTSV(name: string, j: ReturnType<typeof Jsonic.make>) {
  const entries = loadTSV(name)
  for (const { input, expected, row } of entries) {
    if (expected.startsWith('ERROR:')) {
      const code = expected.substring('ERROR:'.length)
      throws(
        () => j(input),
        (err: any) => err.code === code,
        `${name}.tsv row ${row}: expected error ${code} for input=${JSON.stringify(input)}`,
      )
    } else {
      try {
        deepStrictEqual(normalize(j(input)), JSON.parse(expected))
      } catch (err: any) {
        err.message = `${name}.tsv row ${row}: input=${JSON.stringify(input)} expected=${expected}\n${err.message}`
        throw err
      }
    }
  }
}


describe('toml-tsv', () => {

  test('happy', () => runTSV('happy', makeToml()))

  test('basic-values', () => runTSV('basic-values', makeToml()))

  test('integers', () => runTSV('integers', makeToml()))

  test('floats', () => runTSV('floats', makeToml()))

  test('strings', () => runTSV('strings', makeToml()))

  test('arrays', () => runTSV('arrays', makeToml()))

  test('tables', () => runTSV('tables', makeToml()))

  test('dotted-keys', () => runTSV('dotted-keys', makeToml()))

  test('inline-tables', () => runTSV('inline-tables', makeToml()))

  test('array-of-tables', () => runTSV('array-of-tables', makeToml()))

  test('quoted-keys', () => runTSV('quoted-keys', makeToml()))

  test('comments', () => runTSV('comments', makeToml()))

  test('whitespace', () => runTSV('whitespace', makeToml()))

  test('mixed', () => runTSV('mixed', makeToml()))

  test('errors', () => runTSV('errors', makeToml()))

})
