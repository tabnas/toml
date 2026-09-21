// The shared conformance fixtures, every one of them.
//
// `test/spec/*.tsv` is the parity contract: the TypeScript suite
// (`ts/test/toml-tsv.test.ts`), the Go suite (`go/toml_tsv_test.go`
// `TestSpec`) and this file run the same rows. A row green in one runtime
// and red in another is a failure, not a discrepancy.
//
// Discovery is by LISTING the directory, in all three runtimes, so adding
// a `.tsv` runs it everywhere without touching a runner. That is also why
// there is no exemption list here and no tripwire test beside it: nothing
// can be left unrun.

mod common;

use tabnas_support::Runner;

use common::{parse_fresh, spec_dir};

#[test]
fn spec() {
    // The exact code, with no allowances, which is what both other
    // runners assert. `Failure` already carries the code, so the default
    // comparison is the one wanted.
    Runner::new(parse_fresh).dir(spec_dir());
}
