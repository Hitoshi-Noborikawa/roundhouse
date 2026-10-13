//! Backticks on the CRuby overlay; the native twin is
//! `spinel_toolchain/backticks.rs`.

use super::backticks_contract as contract;

/// A backtick literal and `%x{}` run the command, as CRuby runs them.
#[test]
fn backticks_run_the_command() {
    let run = contract::overlay().run_ruby(contract::SCRIPT);
    run.assert_passes();
    assert_eq!(run.stdout, contract::EXPECTED, "stderr:\n{}", run.stderr);
}
