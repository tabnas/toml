# Agents Guide — shared spec fixtures

`spec/*.tsv` holds the cross-runtime conformance fixtures. All three
runtimes run the same files, so a change here affects TypeScript, Go and
Rust together — edit with that in mind.

## Format

Tab-separated, one case per line, with a header row naming the columns
(`input` `expected`). Blank lines are skipped.

| Column | Meaning |
|---|---|
| `input` | TOML source. |
| `expected` | The parse result as JSON. |

### What the loader actually does — mind these

There is one loader now, from `@tabnas/support`, in two languages written
to behave identically. Its
[reference](https://github.com/tabnas/support/blob/main/doc/reference.md)
is the authority; what matters here:

- **Escapes.** `\n`, `\r`, `\t` and `\\` are decoded in the `input` column
  only. A backslash IS writable now — `\\` — where before neither loader
  decoded it and the guidance was to keep backslashes out of fixtures.
  `expected` is raw JSON, which carries its own escape rules and must not
  be decoded twice; it used to be decoded, which was the same cell meaning
  two things depending on which runtime read it.
- **A `#`-leading row is still data** as long as it has a tab — which
  every data row does. That is exactly the rule `comments.tsv` needs, and
  it is the shared loader's rule, not a local quirk. A `#`-leading line
  with NO tab is a comment and is skipped; there are none here.
- Both runtimes split on every tab and read the columns by name from the
  header.

## The divergence register — `test/divergent.tsv`

Separate from `spec/`, and read by `ts/test/divergent.test.ts`,
`go/divergent_test.go` and `rs/tests/divergent_test.rs` rather than by the
shared runner.

It records the places the two ports **disagree**, with a column per port,
and it is **not a fixture**. A fixture fails when behaviour regresses. This
fails **both ways**: when a port is repaired to agree with the other, the
row still claims they differ, so the suite goes red and names the row to
delete. A divergence recorded as a passing test of current behaviour
survives its own repair, with nothing red — which is how the 2026-08 fleet
audit found 29 recorded claims contradicted by execution.

| column | meaning |
|---|---|
| `input` | TOML source, escape-decoded as in `spec/`. |
| `ts`, `go`, `rust` | what each port produces: a JSON value, `ERROR:<code>`, or `ERROR:<code>@<row>:<col>` when the position is the disagreement. |
| `why` | the audit item, and where the repair lives. |

**Position is opt-in.** A cell with no `@row:col` is satisfied by any
position; one that has it is compared on both.

The current two rows are the **astral column unit**, and they are
*permanent*: TypeScript counts UTF-16 code units, so an astral character
advances the column by two, and Go counts runes, so it advances by one.
Rust counts Unicode scalar values, the same unit as a Go rune, so the
`rust` column repeats the `go` one on both rows. There are two answers
here and three ports. Forced by the scan unit and recorded in
`parser/DIVERGENCE.md`. Do not delete them on a sweep — nothing is going to
close them.

They started as four rows of **audit P5**, Go advancing the error column in
*bytes*. That defect is repaired (#51), so the two BMP rows went; the two
astral rows moved with it and only the byte half of their gap closed, so
they were re-attributed rather than deleted. A row whose number changes is
not automatically a row that has served its purpose.

**Measure against the sibling checkouts.** The shared `polyglot-ci`
workflow clones each `tabnas` dependency and wires a `go work use` / npm
symlink over it, so these suites run against `parser`'s `main` — not the
version `go/go.mod` and `ts/package.json` pin. Numbers measured against the
published pin are for a build nothing runs. `admin/scripts/link.sh` sets
this up locally.

This repo had **never been probed cross-runtime** beyond its fixtures: the
fleet probe assumed one Go plugin surface and this repo exports the other,
so it failed to compile and reported "COULD NOT RUN". These are the first
such measurements it has.

The runners are local for now; `tabnas/support#14` makes the mechanism
shared, and the vocabulary here is the one it standardises.

## Who runs what

- TypeScript: `ts/test/toml-tsv.test.ts` — `makeRunner(...).dir(...)`.
- Go: `go/toml_tsv_test.go` — `support.Runner{...}.Dir(t, dir)`.
- Rust: `rs/tests/parity_test.rs` — `Runner::new(...).dir(...)`.

Fixtures are discovered by **listing the directory**, so a new `.tsv` runs
in every runtime at once. They used to be named in a list per runtime, and
a fixture wired into one runtime only would have proved nothing.

An `ERROR:` row pins the error **code** in every runtime, with no allowances.
Go used to accept any rejection, which hid a divergence on `"unterminated`:
`unexpected` in TypeScript, `unterminated_string` in Go. Tightening the
comparison surfaced it; running it down showed TypeScript's string matcher
returning a valid token for an unterminated string — so the `unexpected` came
from the grammar tripping over the one leftover character, not from any
diagnosis, and where nothing was left over malformed TOML parsed silently.
TypeScript is fixed; both runtimes answer `unterminated_string`.

## The conformance counts — `test/conformance.tsv`

Also separate from `spec/`, and read by every runtime's half of the
BurntSushi/toml-test harness: `ts/test/toml.test.ts`,
`go/toml_valid_test.go` and `rs/tests/toml_valid_test.rs`. One row per
runtime, and the numbers are **exact, not floors**, for the reasons that
file's own header gives. The `rust` row reproduces the `ts` row.

## Rules

- Prefer adding a fixture here over a one-off in-language assertion when a
  case is expressible as input → output. That is what keeps the two runtimes
  honest against each other.
- TypeScript is canonical. If the runtimes disagree, the TS behaviour is
  the expected value — unless Go has exposed a genuine TS defect, or the
  difference is one of the intentional divergences the root `AGENTS.md`
  records, which stay out of these shared fixtures.
- A new fixture must pass in EVERY runtime before it counts:
  `go test ./...` from `go/`, `cargo test --all-targets` from `rs/`, and
  **`npm run build && npm test`** from `ts/`. Plain `npm test` runs the
  previously compiled `dist-test/`, so it can pass without ever loading a
  newly added fixture.
