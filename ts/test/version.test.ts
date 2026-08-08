/* Copyright (c) 2026 Richard Rodger, MIT License */

// The exported VERSION must equal package.json "version".
//
// This is the CI check for version drift. It exists because the constant HAS
// drifted: @tabnas/json exported Version = '1.0.0' for several releases while
// the package shipped 0.4.x, because nothing rewrote it and AGENTS.md wrongly
// claimed `make publish-go` kept it in sync. A release that bumps
// package.json and forgets the constant now fails here.

import { test, describe } from 'node:test'
import { equal, match, fail } from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { VERSION } from '..'


// Read package.json at runtime rather than importing it: the test tsconfig
// rootDir is test/, so the JSON cannot be pulled in as a module. Any failure
// to read or parse is a hard test failure, never a skip — a version check
// that silently does not run is the failure mode this test exists to prevent.
function loadPkg(): { name: string; version: string } {
  const pkgPath = join(__dirname, '..', 'package.json')
  let raw: string
  try {
    raw = readFileSync(pkgPath, 'utf8')
  } catch (err: any) {
    return fail(
      `cannot read ${pkgPath}, so VERSION cannot be checked: ${err.message}`,
    )
  }
  try {
    return JSON.parse(raw)
  } catch (err: any) {
    return fail(`${pkgPath} is not readable JSON: ${err.message}`)
  }
}


describe('version', () => {
  test('VERSION matches package.json', () => {
    const pkg = loadPkg()
    if (!pkg.version) {
      fail('package.json has no version field')
    }
    equal(
      VERSION,
      pkg.version,
      `VERSION drift: ${pkg.name} exports ${VERSION} but package.json is ` +
        `${pkg.version}. Both are rewritten by admin/publish.sh at release; ` +
        `if you bumped one by hand, bump the other.`,
    )
  })

  test('VERSION is exported and looks like a semver', () => {
    // Checked through the package root too, because that is how consumers
    // reach it: require('@tabnas/toml').VERSION must resolve.
    const api = require('..')
    equal(typeof api.VERSION, 'string', 'VERSION must be exported as a string')
    match(api.VERSION, /^\d+\.\d+\.\d+/, 'VERSION must be a semver')
  })
})
