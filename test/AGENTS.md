# Agents Guide — shared spec fixtures

`spec/*.tsv` holds the cross-runtime conformance fixtures. Both runtimes run
the same files, so a change here affects TypeScript and Go together — edit
with that in mind.

## Format

Tab-separated, one case per line, with a header row naming the columns
(`input` `expected`). Blank lines are skipped.

| Column | Meaning |
|---|---|
| `input` | TOML source. |
| `expected` | The parse result as JSON. |

### What the loaders actually do — mind these

- **Escapes.** `\n`, `\r`, `\r\n` and `\t` are decoded, in **both** columns.
  A literal `\\` is **not** decoded by the TypeScript loader (Go's leaves it
  alone too), so there is no portable way to write a single backslash — keep
  backslashes out of fixtures.
- **`#` does not start a comment.** Both loaders split every non-blank line
  at the first tab, so a row whose TOML source begins with `#` (a TOML
  comment) is an ordinary data row, which is what `comments.tsv` relies on.
  There is no comment syntax for this file format.
- Go splits at the **first** tab only, TypeScript splits on every tab and
  uses the first two fields; a fixture must therefore have exactly two
  columns.

## Who runs what

- TypeScript: `ts/test/toml-tsv.test.ts`.
- Go: `go/toml_tsv_test.go`.

Both name the same files. A fixture only one runtime runs proves nothing, so
wire a new file into both.

## Rules

- Prefer adding a fixture here over a one-off in-language assertion when a
  case is expressible as input → output. That is what keeps the two runtimes
  honest against each other.
- TypeScript is canonical. If the two runtimes disagree, the TS behaviour is
  the expected value — unless Go has exposed a genuine TS defect, or the
  difference is one of the intentional divergences the root `AGENTS.md`
  records, which stay out of these shared fixtures.
- A new fixture must pass in BOTH runtimes before it counts:
  `go test ./...` from `go/`, and **`npm run build && npm test`** from `ts/`.
  Plain `npm test` runs the previously compiled `dist-test/`, so it can pass
  without ever loading a newly added fixture.
