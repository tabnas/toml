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

Separate from `spec/`, and read by `ts/test/divergent.test.ts` and
`go/divergent_test.go` rather than by the shared runner.

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
| `ts`, `go` | what each port produces: a JSON value, `ERROR:<code>`, or `ERROR:<code>@<row>:<col>` when the position is the disagreement. |
| `why` | the audit item, and where the repair lives. |

**Position is opt-in.** A cell with no `@row:col` is satisfied by any
position; one that has it is compared on both.

The current four rows are all **audit P5** — Go advancing the error column
in *bytes*, so a two-byte `é` costs two columns and a four-byte emoji costs
four. They go red when `tabnas/parser#124` is adopted, which is the signal
to delete them.

This repo had **never been probed cross-runtime** beyond its fixtures: the
fleet probe assumed one Go plugin surface and this repo exports the other,
so it failed to compile and reported "COULD NOT RUN". These are the first
such measurements it has.

The runners are local for now; `tabnas/support#14` makes the mechanism
shared, and the vocabulary here is the one it standardises.

## Who runs what

- TypeScript: `ts/test/toml-tsv.test.ts` — `makeRunner(...).dir(...)`.
- Go: `go/toml_tsv_test.go` — `support.Runner{...}.Dir(t, dir)`.

Fixtures are discovered by **listing the directory**, so a new `.tsv` runs
in both runtimes at once. They used to be named in a list per runtime, and
a fixture wired into one runtime only would have proved nothing.

An `ERROR:` row pins the error **code** in both runtimes, with no allowances.
Go used to accept any rejection, which hid a divergence on `"unterminated`:
`unexpected` in TypeScript, `unterminated_string` in Go. Tightening the
comparison surfaced it; running it down showed TypeScript's string matcher
returning a valid token for an unterminated string — so the `unexpected` came
from the grammar tripping over the one leftover character, not from any
diagnosis, and where nothing was left over malformed TOML parsed silently.
TypeScript is fixed; both runtimes answer `unterminated_string`.

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
