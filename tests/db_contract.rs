//! The Db contract suite on the CRuby shims (runtime/spinel/db_cruby.rb
//! today), and the lane table every shim is checked from.
//!
//! The suite, its layout and how to point a new shim at it are described
//! in tests/support/db_contract.rs. The Spinel lanes run from
//! tests/spinel_db_contract.rs (SQLite) and tests/spinel_pg_db.rs
//! (PostgreSQL), which need the Spinel toolchain.
//!
//!   cargo test --test db_contract

#[path = "support/db_contract.rs"]
mod db_contract;

use db_contract::{Backend, LANES, Runtime, lanes};

#[test]
fn cruby_sqlite_shims_pass_the_contract() {
    let mut ran = 0;
    for lane in lanes(Runtime::Cruby, Backend::Sqlite) {
        db_contract::run_lane(lane);
        ran += 1;
    }
    assert!(ran > 0, "no CRuby SQLite lane");
}

/// Every Ruby-family `Db` shim the runtime ships is a lane, except the
/// JRuby one, which needs a JVM (compare-jruby covers it end to end).
#[test]
fn every_ruby_family_shim_has_a_lane() {
    let mut shims: Vec<String> = std::fs::read_dir("runtime/spinel")
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| (n == "db.rb" || n.starts_with("db_")) && n.ends_with(".rb"))
        .filter(|n| n != "db_jruby.rb")
        .map(|n| format!("runtime/spinel/{n}"))
        .collect();
    shims.sort();
    let mut laned: Vec<String> = LANES.iter().map(|l| l.shim.to_string()).collect();
    laned.sort();
    laned.dedup();
    assert_eq!(
        shims, laned,
        "a Db shim with no contract lane, or a lane with no shim"
    );
    for lane in LANES {
        assert!(
            lane.backend == Backend::Sqlite || lane.lane_file.is_some(),
            "the PostgreSQL lane {} names no lane file",
            lane.name
        );
    }
}
