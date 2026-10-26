//! The Spinel half of `emit_and_run/data_paths.rs`.

use super::data_paths_contract as contract;

#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn lobsters_data_path_constructs_run_natively() {
    let run = contract::overlay().run_spinel_with_db(contract::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("data paths contract passed"), "{}", run.stdout);
}
