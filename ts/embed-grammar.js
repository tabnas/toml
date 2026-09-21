#!/usr/bin/env node

// Embeds toml-grammar.jsonic into src/toml.ts, go/toml.go and
// rs/src/lib.rs.
// Run via: npm run embed
//
// Every runtime holds the SAME grammar text and parses it with its own
// jsonic at load time. Never hand-edit between the BEGIN/END markers:
// edit toml-grammar.jsonic and re-run this script.

const fs = require('fs')
const path = require('path')

const grammar = fs.readFileSync(path.join(__dirname, '..', 'toml-grammar.jsonic'), 'utf8')

const BEGIN = '// --- BEGIN EMBEDDED toml-grammar.jsonic ---'
const END = '// --- END EMBEDDED toml-grammar.jsonic ---'

function embed(file, wrapContent) {
  let src = fs.readFileSync(file, 'utf8')
  const beginIdx = src.indexOf(BEGIN)
  const endIdx = src.indexOf(END)
  if (beginIdx === -1 || endIdx === -1) {
    console.error('Error: embedding markers not found in ' + file)
    process.exit(1)
  }
  const replacement = BEGIN + '\n' + wrapContent + '\n' + END
  src = src.substring(0, beginIdx) + replacement + src.substring(endIdx + END.length)
  fs.writeFileSync(file, src)
}

// TypeScript: template literal (escape backslashes, backticks, ${).
const tsContent = grammar
  .replace(/\\/g, '\\\\')
  .replace(/`/g, '\\`')
  .replace(/\$\{/g, '\\${')
embed(
  path.join(__dirname, 'src', 'toml.ts'),
  'const grammarText = `\n' + tsContent + '`'
)

// Go: raw string (backticks cannot appear in content).
if (grammar.includes('`')) {
  console.error('Error: grammar file contains backticks, cannot embed in Go raw string')
  process.exit(1)
}
// The trailing newline leaves a blank line before END, which keeps the
// result gofmt-clean (gofmt wants a blank line between the const
// declaration and the trailing comment).
embed(
  path.join(__dirname, '..', 'go', 'toml.go'),
  'const grammarText = `\n' + grammar + '`\n'
)

// Rust: raw string (no escapes at all). An `r#"` literal ends at the first
// `"#`, so that sequence is the one thing the content may not contain.
// The grammar quotes with `'`, so it holds no `"` at all today; the guard
// is here so a future edit fails loudly rather than truncating the const.
const RS_FILE = path.join(__dirname, '..', 'rs', 'src', 'lib.rs')
if (fs.existsSync(RS_FILE)) {
  if (grammar.includes('"#')) {
    console.error('Error: grammar file contains `"#`, cannot embed in a Rust r#"..."# literal')
    process.exit(1)
  }
  embed(RS_FILE, 'pub(crate) const GRAMMAR_TEXT: &str = r#"\n' + grammar + '"#;')
} else {
  console.log('No Rust source at', RS_FILE, '- skipping')
}
