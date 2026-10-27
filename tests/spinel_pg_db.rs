//! The Spinel `Db` shim over spinel-pg (runtime/spinel/db_pg.rb).
//!
//! The gate compiles the Db contract suite (tests/db_contract/) next to
//! the shim with Spinel, seeded with the shim's RBS, and runs it against
//! a live PostgreSQL: the contract cases every shim passes, the cases
//! any PostgreSQL Db passes, and db_pg.rb's own (its lane file,
//! tests/db_contract/db_pg_lane.rb). Nothing selects the shim for an
//! emitted app yet, so the suite drives the `Db` surface directly. The
//! suite and its lanes are described in tests/support/db_contract.rs.
//!
//!   SPINEL_PG_DIR=/path/to/spinel-pg \
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres:secret@127.0.0.1:5432/postgres \
//!   cargo test --test spinel_pg_db -- --ignored --nocapture
//!
//! Without ROUNDHOUSE_PG_TEST_URL the gate says so and skips, unless
//! ROUNDHOUSE_PG_TEST_REQUIRED is set, as CI sets it; a missing
//! SPINEL_PG_DIR fails it. Each run gets a schema of its own
//! (tests/support/pg_test_url.rs), dropped when it finishes.

#[path = "support/db_contract.rs"]
mod db_contract;

use db_contract::{Backend, Runtime, lanes};
use std::collections::BTreeSet;

#[test]
#[ignore = "requires Spinel, SPINEL_PG_DIR (a spinel-pg checkout) and ROUNDHOUSE_PG_TEST_URL (PostgreSQL)"]
fn spinel_pg_shims_pass_the_contract_and_postgres_cases() {
    let mut ran = 0;
    for lane in lanes(Runtime::Spinel, Backend::Postgres) {
        db_contract::run_lane(lane);
        ran += 1;
    }
    assert!(ran > 0, "no Spinel PostgreSQL lane");
}

/// `(owner, name)` for every `def` in a Ruby or RBS source, with `self.`
/// kept on singleton methods. Owners are the top-level `class`/`module`
/// lines, which is all these two files use.
fn definitions(src: &str, rbs: bool) -> BTreeSet<(String, String)> {
    let mut owner = String::new();
    let mut out = BTreeSet::new();
    for line in src.lines() {
        for keyword in ["class ", "module "] {
            if let Some(rest) = line.strip_prefix(keyword) {
                owner = rest.split_whitespace().next().unwrap_or("").to_string();
            }
        }
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("def ") else {
            continue;
        };
        let end = rest
            .find(|c: char| {
                if rbs {
                    c == ':'
                } else {
                    c == '(' || c.is_whitespace()
                }
            })
            .unwrap_or(rest.len());
        out.insert((owner.clone(), rest[..end].trim().to_string()));
    }
    out
}

/// Every method the shim (db_pg.rb, pg_errors.rb) defines carries a
/// signature: the contract in runtime/ruby/db.rbs, or the shim's own
/// db_pg.rbs / pg_errors.rbs. All of them parse as RBS.
#[test]
fn db_pg_rbs_declares_every_method() {
    let read =
        |path: &str| std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let contract = read("runtime/ruby/db.rbs");
    let mut ruby = String::new();
    let mut own = String::new();
    for stem in ["runtime/spinel/db_pg", "runtime/spinel/pg_errors"] {
        ruby.push_str(&read(&format!("{stem}.rb")));
        ruby.push('\n');
        let rbs = read(&format!("{stem}.rbs"));
        roundhouse::rbs::parse_app_signatures(&rbs)
            .unwrap_or_else(|e| panic!("{stem}.rbs does not parse: {e}"));
        own.push_str(&rbs);
        own.push('\n');
    }
    roundhouse::rbs::parse_app_signatures(&contract)
        .unwrap_or_else(|e| panic!("runtime/ruby/db.rbs does not parse: {e}"));
    let mut declared = definitions(&own, true);
    declared.extend(definitions(&contract, true));
    let defined = definitions(&ruby, false);
    assert!(
        defined.len() > 60,
        "found only {} definitions",
        defined.len()
    );
    let missing: Vec<String> = defined
        .difference(&declared)
        .map(|(owner, name)| format!("{owner}#{name}"))
        .collect();
    assert!(
        missing.is_empty(),
        "the PostgreSQL shim defines methods with no RBS signature:\n  {}",
        missing.join("\n  ")
    );
    let stale: Vec<String> = definitions(&own, true)
        .difference(&defined)
        .map(|(owner, name)| format!("{owner}#{name}"))
        .collect();
    assert!(
        stale.is_empty(),
        "the PostgreSQL shim RBS declares methods the shim does not define:\n  {}",
        stale.join("\n  ")
    );
}
