/* Copyright (c) 2021-2025 Richard Rodger, MIT License */

// Import Jsonic types used by plugin.
import { Jsonic, Rule, Lex, Plugin, EMPTY } from 'jsonic'

// See defaults below for commentary.
type TomlOptions = {}

// --- BEGIN EMBEDDED toml-grammar.jsonic ---
const grammarText = `
# TOML Grammar Definition
# Parsed by a standard Jsonic instance and passed to jsonic.grammar()
# Function references (@ prefixed) are resolved against the refs map.
# Regex references (@/pattern/flags) are resolved to RegExp instances.

{
  options: rule: { start: toml exclude: jsonic }
  options: lex: {
    emptyResult: {}
    match: string: make: '@make-toml-string-matcher'
  }
  options: fixed: token: { '#CL': '=' '#DOT': '.' }
  options: match: {
    token: { '#ID': '@/^[a-zA-Z0-9_-]+/' }
    value: {
      isodate: {
        match: '@/^\\\\d\\\\d\\\\d\\\\d-\\\\d\\\\d-\\\\d\\\\d([Tt ]\\\\d\\\\d:\\\\d\\\\d(:\\\\d\\\\d(\\\\.\\\\d+)?)?([Zz]|[-+]\\\\d\\\\d:\\\\d\\\\d)?)?/'
        val: '@isodate-val'
      }
      localtime: {
        match: '@/^\\\\d\\\\d:\\\\d\\\\d(:\\\\d\\\\d(\\\\.\\\\d+)?)?/'
        val: '@localtime-val'
      }
    }
  }
  options: tokenSet: {
    KEY: ['#ST' '#ID' null null]
  }
  options: comment: def: { slash: null multi: null }

  rule: toml: open: [
    { s: ['#ST #NR #ID' '#CL'] p: table b: 2 }
    { s: ['#OS' '#ST #NR #ID'] p: table b: 2 }
    { s: ['#OS' '#OS'] p: table b: 2 }
    { s: ['#ST #NR #ID' '#DOT'] p: table b: 2 }
    { s: '#ZZ' }
  ]

  rule: table: {
    open: [
      { s: ['#ST #NR #ID' '#CL'] p: map b: 2 }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 }
      { s: ['#OS' '#OS'] r: table n: { table_array: 1 } }
      {
        s: ['#ST #NR #ID' '#DOT']
        c: '@table-top-dive-cond'
        p: dive
        b: 2
        u: { top_dive: true }
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        c: '@lte-table-dive'
        n: { table_dive: 1 }
        a: '@table-dive-start'
        g: 'dive,start'
      }
      {
        s: ['#ST #NR #ID' '#DOT']
        r: table
        n: { table_dive: 1 }
        a: '@table-dive-mid'
        g: 'dive'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        c: '@lte-table-dive'
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-head'
      }
      {
        s: ['#ST #NR #ID' '#CS']
        p: '@table-end-p'
        r: '@table-end-r'
        a: '@table-key-cs-tail'
        g: 'dive,end'
      }
      {
        s: '#CS'
        p: map
        c: '@lte-table-array-1'
        a: '@table-cs-push'
      }
    ]
    close: [
      { s: ['#OS' '#OS'] r: table b: 2 }
      { s: ['#OS' '#ST #NR #ID'] r: table b: 1 }
      { s: '#ZZ' }
    ]
  }

  rule: map: {
    open: [
      { s: '#OS' b: 1 }
      {
        s: ['#ST #NR #ID' '#CL']
        c: '@map-is-table-parent'
        p: pair
        b: 2
      }
      { s: ['#OB' '#ST #NR #ID'] b: 1 p: pair }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
      { s: '#ZZ' }
    ]
    close: [
      { s: '#OS' b: 1 }
      { s: '#ZZ' }
    ]
  }

  rule: pair: {
    open: [
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        u: { pair: true }
        a: '@pair-key-set'
      }
      { s: ['#ST #NR #ID' '#DOT'] p: dive b: 2 }
    ]
    close: [
      { s: ['#ST #NR #ID'] b: 1 r: pair }
      { s: ['#CA' '#ST #NR #ID'] b: 1 r: pair }
      { s: ['#OS'] b: 1 }
      { s: ['#CA' '#CB'] c: '@lte-pk' b: 1 }
    ]
  }

  rule: val: close: [
    { s: ['#ST #NR #ID'] b: 1 }
    { s: ['#OS'] b: 1 }
  ]

  rule: elem: close: [
    { s: ['#CA' '#CS'] b: 1 g: comma }
  ]

  rule: dive: {
    open: [
      {
        s: ['#ST #NR #ID' '#DOT']
        p: dive
        n: { dive_key: 1 }
        a: '@dive-key-dot'
      }
      {
        s: ['#ST #NR #ID' '#CL']
        p: val
        n: { dive_key: 1 }
        u: { dive_end: true }
      }
    ]
    close: [
      {
        s: ['#ST #NR #ID' '#DOT']
        b: 2
        r: dive
        c: '@lte-dive-key-1'
        n: { dive_key: 0 }
      }
      {}
    ]
  }
}
`
// --- END EMBEDDED toml-grammar.jsonic ---

// Plugin implementation.
const Toml: Plugin = (jsonic: Jsonic, _options: TomlOptions) => {
  // Named function references used by the declarative grammar.
  const refs: Record<string, any> = {
    // Options callbacks.
    '@make-toml-string-matcher': makeTomlStringMatcher,

    // Referenced by the embedded grammar's `val:` fields, used by the Go
    // port which keeps the regex-based matchers. The TS plugin replaces
    // the whole matcher with @isodate-match / @localtime-match below, so
    // these aren't actually invoked on this side — kept only so
    // jsonic.grammar() can resolve the '@isodate-val' / '@localtime-val'
    // refs during option installation.
    '@isodate-val': (res: any) => {
      const date: any = new Date(res[0])
      date.__toml__ = {
        kind:
          (null == res[4] ? 'local' : 'offset') +
          '-date' +
          (null == res[1] ? '' : '-time'),
        src: res[0],
      }
      return date
    },

    '@localtime-val': (res: any) => {
      const date: any = new Date(
        60 * 60 * 1000 + new Date('1970-01-01 ' + res[0]).getTime(),
      )
      date.__toml__ = { kind: 'local-time', src: res[0] }
      return date
    },

    // Context-aware replacements installed after jsonic.grammar() runs.
    // A bare date-shaped key like `2001-02-03 = 1` or a table header
    // `[2002-01-02]` would otherwise be swallowed by the regex value
    // matcher before the #ID token matcher ever gets a chance.
    '@isodate-match': (lex: Lex, rule: any) => {
      if (isKeyContext(lex, rule)) return null
      const m = lex.fwd.match(
        /^\d\d\d\d-\d\d-\d\d([Tt ]\d\d:\d\d(:\d\d(\.\d+)?)?([Zz]|[-+]\d\d:\d\d)?)?/,
      )
      if (!m) return null
      const date: any = new Date(m[0])
      date.__toml__ = {
        kind:
          (null == m[4] ? 'local' : 'offset') +
          '-date' +
          (null == m[1] ? '' : '-time'),
        src: m[0],
      }
      const pnt = lex.pnt
      const tkn = lex.token('#VL', date, m[0], pnt)
      pnt.sI += m[0].length
      pnt.cI += m[0].length
      return tkn
    },

    '@localtime-match': (lex: Lex, rule: any) => {
      if (isKeyContext(lex, rule)) return null
      const m = lex.fwd.match(/^\d\d:\d\d(:\d\d(\.\d+)?)?/)
      if (!m) return null
      const date: any = new Date(
        60 * 60 * 1000 + new Date('1970-01-01 ' + m[0]).getTime(),
      )
      date.__toml__ = { kind: 'local-time', src: m[0] }
      const pnt = lex.pnt
      const tkn = lex.token('#VL', date, m[0], pnt)
      pnt.sI += m[0].length
      pnt.cI += m[0].length
      return tkn
    },

    // State actions (auto-applied by fnref via @<rule>-<state> convention).
    '@toml-bo': (r: Rule) => {
      r.node = {}
    },

    '@table-bo': (r: Rule) => {
      r.node = r.parent.node
    },

    '@table-bc': (r: Rule) => {
      if (!r.u.top_dive) {
        Object.assign(r.node, r.child.node)
      }
    },

    '@table-ac': (_r: Rule, _ctx: any, next: any) => {
      next.n.table_dive = 0
      next.n.table_array = 0
    },

    '@dive-bc': (r: Rule) => {
      if (r.u.dive_end) {
        r.node[r.o0.val] = r.child.node
      }
    },

    // Alt actions.
    '@table-dive-start': (r: any) => {
      let key = r.o0.val
      if (r.n.table_array && Array.isArray(r.parent.node[key])) {
        let arr = r.parent.node[key]
        let last = arr[arr.length - 1]
        r.node = last ? last : (arr.push({}), arr[arr.length - 1])
      } else {
        r.node = r.parent.node[key] = r.parent.node[key] || {}
      }
    },

    '@table-dive-mid': (r: any) => {
      let key = r.o0.val
      if (Array.isArray(r.prev.node)) {
        let arr = r.prev.node
        let last = arr[arr.length - 1]
        last = last ? last : (arr.push({}), arr[arr.length - 1])
        r.node = last[key] = last[key] || {}
      } else {
        r.node = r.prev.node[key] = r.prev.node[key] || {}
      }
    },

    '@table-key-cs-head': (r: any) => {
      let key = r.o0.val
      r.parent.node[key] = r.node =
        r.parent.node[key] || (r.n.table_array ? [] : {})
    },

    '@table-key-cs-tail': (r: any) => {
      let key = r.o0.val
      if (Array.isArray(r.prev.node)) {
        let arr = r.prev.node
        let last = arr[arr.length - 1]
        last = last ? last : (arr.push({}), arr[arr.length - 1])
        r.node = last[key] = last[key] || {}
      } else {
        r.node = r.prev.node[key] =
          r.prev.node[key] || (r.n.table_array ? [] : {})
      }
    },

    '@table-cs-push': (r: any) => {
      r.prev.node.push((r.node = {}))
    },

    '@pair-key-set': (r: Rule) => {
      r.u.key = r.o0.val
    },

    '@dive-key-dot': (r: any) => {
      r.parent.node[r.o0.val] = r.node = r.parent.node[r.o0.val] || {}
    },

    // Conditions.
    '@table-top-dive-cond': (r: any) => 1 === r.d && 'table' !== r.prev.name,
    '@lte-table-dive': (r: any) => r.lte('table_dive'),
    '@lte-table-array-1': (r: any) => r.lte('table_array', 1),
    '@lte-dive-key-1': (r: any) => r.lte('dive_key', 1),
    '@lte-pk': (r: any) => r.lte('pk'),
    '@map-is-table-parent': (r: any) => 'table' === r.parent.name,

    // Conditional next-rule targets (p:/r: returning a rule name or false).
    '@table-end-p': (r: any) => !r.n.table_array && 'map',
    '@table-end-r': (r: any) => r.n.table_array && 'table',
  }

  // Parse embedded grammar definition using a separate standard Jsonic instance,
  // then apply options and rules to this plugin's Jsonic.
  const grammarDef: any = Jsonic.make()(grammarText)
  grammarDef.ref = refs

  // Patch option values that can't be expressed in the grammar text
  // (NaN/Infinity literals can't round-trip through Jsonic parsing).
  grammarDef.options.value = {
    def: {
      nan: { val: NaN },
      '+nan': { val: NaN },
      '-nan': { val: NaN },
      inf: { val: Infinity },
      '+inf': { val: Infinity },
      '-inf': { val: -Infinity },
    },
  }

  jsonic.grammar(grammarDef)

  // Swap the grammar's regex-based date/time matchers for the
  // context-aware function matchers. The grammar file keeps the regex
  // form so the Go port (which has no equivalent of isKeyContext) still
  // parses it. On the TS side, these overrides let date-shaped bare keys
  // fall through to the #ID token matcher.
  jsonic.options({
    match: {
      value: {
        isodate: { match: refs['@isodate-match'] },
        localtime: { match: refs['@localtime-match'] },
      },
    },
  })
}

// Value matchers fire unconditionally, so a date-shaped bare key
// (e.g. `2001-02-03 = 1`, `[2002-01-02]`, `a.2001-02-08 = 7`) would be
// claimed as a datetime value unless we defer to the #ID token matcher
// when the current rule position accepts a key. Value-producing rules
// (val, list, elem) never list #ID in their expected tokens; key-accepting
// rules (toml, map, dive, pair, table) do. Value matchers aren't told which
// tI they're at, so we scan all positions in the current state.
function isKeyContext(lex: Lex, rule: any): boolean {
  const tcol = rule?.spec?.def?.tcol
  if (!tcol) return false
  const oc = 'o' === rule.state ? 0 : 1
  const positions = tcol[oc]
  if (!positions) return false
  const idTin = lex.tokenize('#ID')
  for (const expected of positions) {
    if (expected && expected.includes(idTin)) return true
  }
  return false
}

// Adapted from https://github.com/huan231/toml-nodejs/blob/master/src/tokenizer.ts
// Copyright (c) 2022 Jan Szybowski, MIT License
function makeTomlStringMatcher() {
  return function stringMatcher(lex: Lex) {
    let { pnt, src } = lex
    let { sI, rI, cI } = pnt
    let srclen = src.length

    let isMultiline = false
    let begin = sI

    let delimiter = src[sI]
    let singleQuote = "'" === delimiter
    let doubleQuote = '"' === delimiter

    if (!singleQuote && !doubleQuote) {
      return
    }

    if (delimiter === src[sI + 1]) {
      if (delimiter !== src[sI + 2]) {
        pnt.sI = sI + 2
        pnt.cI = cI + 2
        return lex.token('#ST', EMPTY, EMPTY, pnt)
      }

      sI += 2
      cI += 2
      isMultiline = true
    }

    // A newline immediately following the opening delimiter will be trimmed.
    // https://toml.io/en/v1.0.0#string
    if (isMultiline) {
      if ('\n' === src[sI + 1]) {
        ++sI
        cI = 0
      }
    }

    let value = ''

    for (; sI < srclen - 1; ) {
      ++sI
      ++cI

      const char = src[sI]

      switch (char) {
        case '\n':
          if (!isMultiline) {
            return lex.bad('unprintable', sI, sI + 1)
          }

          value += char
          cI = 0
          ++rI
          continue

        case delimiter:
          if (isMultiline) {
            if (delimiter !== src[sI + 1]) {
              value += delimiter
              continue
            }

            if (delimiter !== src[sI + 2]) {
              value += delimiter
              value += delimiter
              cI += 1
              sI += 1
              continue
            }

            cI += 2
            sI += 2

            if (delimiter === src[sI + 1]) {
              value += delimiter
              sI++
            }

            if (delimiter === src[sI + 1]) {
              value += delimiter
              sI++
            }
          }

          ++cI
          ++sI

          break

        case undefined:
          return lex.bad('unterminated_string', begin, sI)

        default:
          if (sI >= srclen) {
            return lex.bad('unterminated_string', begin, sI)
          }

          if (
            !isUnicodeCharacter(char) ||
            isControlCharacterOtherThanTab(char)
          ) {
            return lex.bad('unprintable', sI, sI + 1)
          }

          switch (delimiter) {
            case "'":
              value += char

              continue

            case '"':
              if (char === '\\') {
                const char = src[(++cI, ++sI)]

                if (isEscaped(char)) {
                  value += ESCAPES[char]

                  continue
                } else if (char === 'x') {
                  sI++
                  let cc = parseInt(src.substring(sI, sI + 2), 16)

                  if (isNaN(cc)) {
                    sI = sI - 2
                    cI -= 2
                    pnt.sI = sI
                    pnt.cI = cI
                    return lex.bad('invalid_ascii', sI, sI + 4)
                  }

                  let us = String.fromCharCode(cc)

                  value += us
                  sI += 1 // Loop increments sI.
                  cI += 2

                  continue
                }
                // Any Unicode character may be escaped
                // with the \uXXXX or \UXXXXXXXX forms.
                // The escape codes must be valid Unicode scalar values.
                // https://toml.io/en/v1.0.0#string
                else if (char === 'u' || char === 'U') {
                  let beginUnicode = sI
                  const size = char === 'u' ? 4 : 8

                  let codePoint = ''

                  for (let i = 0; i < size; i++) {
                    const char = src[(++cI, ++sI)]

                    if (sI >= srclen || !isHexadecimal(char)) {
                      return lex.bad('invalid_unicode', beginUnicode, sI)
                    }

                    codePoint += char
                  }

                  const result = String.fromCodePoint(parseInt(codePoint, 16))

                  if (!isUnicodeCharacter(result)) {
                    return lex.bad('invalid_unicode', beginUnicode, sI)
                  }

                  value += result

                  continue
                }

                // For writing long strings without introducing
                // extraneous whitespace, use a "line ending
                // backslash".  When the last non-whitespace character
                // on a line is an unescaped \, it will be trimmed
                // along with all whitespace (including newlines) up
                // to the next non-whitespace character or closing
                // delimiter.
                // https://toml.io/en/v1.0.0#string
                if (
                  isMultiline &&
                  (isWhitespace(char) || char === '\n' || char === '\r')
                ) {
                  while (
                    (' ' === src[sI + 1] && ++cI) ||
                    ('\t' === src[sI + 1] && ++cI) ||
                    ('\n' === src[sI + 1] && ((cI = 0), ++rI)) ||
                    ('\r' === src[sI + 1] &&
                      '\n' === src[sI + 2] &&
                      ((cI = 0), ++sI, ++rI))
                  ) {
                    sI++
                  }

                  continue
                }

                value += '\u001b'
                continue
              }

              value += char
              continue
          }
      }

      break
    }

    pnt.sI = sI
    pnt.cI = cI
    pnt.rI = rI

    let st = lex.token('#ST', value, src.substring(begin, sI), pnt)

    return st
  }
}

const ESCAPES = {
  b: '\b',
  t: '\t',
  n: '\n',
  f: '\f',
  r: '\r',
  '"': '"',
  '\\': '\\',
}

const isEscaped = (char: string): char is keyof typeof ESCAPES => {
  return char in ESCAPES
}

const isUnicodeCharacter = (char: string) => {
  // Compare by code point, not UTF-16 lexicographic order: '\u{10ffff}'
  // encodes as the surrogate pair '􏿿', so a literal <= compare
  // wrongly rejects single code units in [, \udbff). It also must
  // accept lone surrogates so characters outside the BMP (iterated as two
  // code units) don't trip an "unprintable" error.
  return (char.codePointAt(0) ?? 0) <= 0x10ffff
}

const isControlCharacter = (char: string) => {
  return ('\u{0}' <= char && char < '\u{20}') || char === '\u{7f}'
}

const isControlCharacterOtherThanTab = (char: string) => {
  return isControlCharacter(char) && char !== '\t'
}

export const isHexadecimal = (char: string) => {
  return (
    ('A' <= char && char <= 'Z') ||
    ('a' <= char && char <= 'z') ||
    ('0' <= char && char <= '9')
  )
}

const isWhitespace = (char: string) => {
  return char === ' ' || char === '\t'
}

// Default option values.
Toml.defaults = {} as TomlOptions

export { Toml }

export type { TomlOptions }
