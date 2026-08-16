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
