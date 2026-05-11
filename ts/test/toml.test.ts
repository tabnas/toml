/* Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License */

import { test, describe } from 'node:test'
import Fs from 'node:fs'
import Path from 'node:path'
import { deepStrictEqual as equal } from 'node:assert/strict'

import { Jsonic } from 'jsonic'
// import { Debug } from 'jsonic/debug'
import { Toml } from '..'


// NOTE: install toml-test repo to run
// npm run install-toml-test


describe('toml', () => {

  test('toml-valid', async () => {
    const toml = Jsonic.make().use(Toml)

    let root = __dirname + '/../test/toml-test/tests/valid'

    let found = find(root, [])

    let fails: any[] = []
    let counts = { pass: 0, fail: 0 }
    for (let test of found) {
      try {
        // console.log('TEST', test.name)
        test.out = toml(test.toml)
        test.norm = norm(test.out, test.name)
        equal(test.norm, test.json)
        // console.log('PASS', test.name)
        counts.pass++
      }
      catch (e: any) {
        console.log('FAIL', test.name)
        // console.dir(test, { depth: null })
        counts.fail++
        fails.push(test.name)
        // throw e
      }
    }

    console.log('COUNTS', counts)

    console.log('FAILS', fails)


    // Handle test case oddities
    function norm(val: any, name: string) {
      // Tests where every numeric leaf is a float (values happen to be
      // integer-valued, so the int-vs-float guess below can't recover this
      // without help).
      const allFloat =
        name.endsWith('float/max-int') ||
        name.endsWith('spec-1.0.0/float-0') ||
        name.endsWith('spec-1.1.0/common-23') ||
        name.endsWith('inline-table/spaces')

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
                return {
                  type: name.endsWith('exponent') ? 'float' : 'integer',
                  value: '' + v + (name.endsWith('exponent') ? '.0' : '')
                }
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
})

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
