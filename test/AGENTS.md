# Agents Guide — shared spec fixtures

`spec/*.tsv` holds the cross-runtime conformance fixtures. Both runtimes run
the same files, so a change here affects TypeScript and Go together — edit
with that in mind.

## Format

Tab-separated, one case per line, with a header row naming the columns
(`input` `expected`).

| Column | Meaning |
|---|---|
| `input` | TOML source. Escapes `\n` `\r` `\t` `\\` are decoded. |
| `expected` | The parse result as JSON, or `ERROR:<code>` for input that must be rejected. |

`expected` is **not** escape-decoded — it is raw JSON, so JSON's own escape
rules apply. A row whose input begins with `#` is still a data row (a
comment line is one with no tab at all), which matters here because `#`
starts a comment in the source language too.

## Who runs what

- TypeScript: `ts/test/toml-tsv.test.ts`.
- Go: `go/toml_tsv_test.go`.

Both name the same files. A fixture that only one runtime runs proves
nothing, so wire a new file into both.

## Rules

- Prefer adding a fixture here over a one-off in-language assertion when a
  case is expressible as input → output. That is what keeps the two runtimes
  honest against each other.
- TypeScript is canonical. If the two runtimes disagree, the TS behaviour is
  the expected value — unless Go has exposed a genuine TS defect, in which
  case fix TS first and pin the corrected behaviour here.
- A new fixture must pass in BOTH runtimes: run `go test ./...` (from `go/`)
  and `npm test` (from `ts/`) before considering it done.
