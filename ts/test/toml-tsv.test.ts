/* Copyright (c) 2022-2026 Richard Rodger and other contributors, MIT License */

// Cross-runtime conformance, driven by the shared `test/spec/*.tsv` fixtures
// at the repo root (see ../../test/AGENTS.md).
//
// The fixture loader, the escape codec, the `ERROR:<code>` contract and the
// row loop all come from @tabnas/support, whose Go half `go/toml_tsv_test.go`
// uses to run the SAME files — so the two implementations cannot drift
// without one of them going red, and neither can the two loaders.
//
// What is left here is only what is specific to toml: how to build the
// parser.

import { Tabnas } from '@tabnas/parser'
import { jsonic } from '@tabnas/jsonic'
import { findSpecDir, makeRunner } from '@tabnas/support'

import { Toml } from '..'

makeRunner({
  // toml takes no per-row options, but a fresh parser per row keeps one
  // case's table state from reaching the next.
  parse: (input) => new Tabnas().use(jsonic).use(Toml).parse(input),
})
  // `findSpecDir` walks up from this file — `dist-test/` at runtime — to the
  // repo root's `test/spec`, so moving the suite does not mean recounting
  // `..` hops. `dir` then auto-discovers every fixture in it, so adding a
  // .tsv runs it in both runtimes without touching either runner — it used
  // to have to be named in a list, once per runtime.
  .dir(findSpecDir(__dirname))
