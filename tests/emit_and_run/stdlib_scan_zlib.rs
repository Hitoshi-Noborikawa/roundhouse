use super::stdlib_scan_zlib_contract as contract;

#[test]
fn string_scanner_math_pi_and_zlib_run_after_emission() {
    contract::app().run_ruby(contract::ASSERTIONS).assert_passes();
}
