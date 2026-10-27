//! The Db contract suite's lanes: which shim, on which runtime, against
//! which database, and the runner that copies one lane into a scratch
//! directory and runs it.
//!
//! The suite itself is Ruby, in tests/db_contract/:
//!
//!   contract_cases.rb  — the Db contract (runtime/ruby/db.rbs), every lane
//!   postgres_cases.rb  — what any PostgreSQL Db adds, PostgreSQL lanes
//!   <shim>_lane.rb     — a PostgreSQL lane's hook and its shim's own cases
//!   postgres_main.rb   — the program a PostgreSQL lane runs
//!
//! A lane copies its shim in as `db.rb`, the runtime's prelude
//! (`spinel_prelude.rb`) as `prelude.rb`, and the lane file as `lane.rb`,
//! then compiles the backend's main file with Spinel. Each suite prints
//! `<suite>: N checks passed`, and every lane must report exactly the
//! counts below, so a lane that skipped a case fails rather than passes.
//!
//! Pointing a new shim at the suite is one row in `LANES`. The tests that
//! run lanes select them by runtime and backend, so the row is picked up
//! by tests/spinel_pg_db.rs. A PostgreSQL row names a lane file;
//! db_pg_lane.rb shows the one hook it must define.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Checks in contract_cases.rb; every lane runs all of them.
pub const CONTRACT_CHECKS: usize = 129;
/// Checks in postgres_cases.rb; every PostgreSQL lane runs all of them.
pub const POSTGRES_CHECKS: usize = 45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    Cruby,
    Spinel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sqlite,
    Postgres,
}

pub struct Lane {
    /// Names the lane in output and scratch paths.
    pub name: &'static str,
    /// The `Db` shim, copied in as db.rb.
    pub shim: &'static str,
    pub runtime: Runtime,
    pub backend: Backend,
    /// Runtime files the shim requires, copied beside it by basename.
    pub requires: &'static [&'static str],
    /// RBS handed to Spinel with `--rbs`, copied into sig/.
    pub rbs: &'static [&'static str],
    /// A directory the compile adds with `-I`, named by this variable
    /// (a Spinel package checkout). Required when set.
    pub include_env: Option<(&'static str, &'static str)>,
    /// PostgreSQL lanes: the lane file (hook and shim-own cases), and
    /// the checks it reports under its `lane_name`.
    pub lane_file: Option<&'static str>,
    pub lane_checks: usize,
}

pub const LANES: &[Lane] = &[Lane {
    name: "db_pg",
    shim: "runtime/spinel/db_pg.rb",
    runtime: Runtime::Spinel,
    backend: Backend::Postgres,
    requires: &[
        "runtime/spinel/pg_errors.rb",
        "runtime/spinel/active_support_time_parsing.rb",
        "runtime/spinel/active_support_interval.rb",
        "runtime/spinel/active_support_duration.rb",
    ],
    rbs: &[
        "runtime/spinel/db_pg.rbs",
        "runtime/spinel/pg_errors.rbs",
        "runtime/ruby/db.rbs",
        "runtime/spinel/active_support_time_parsing.rbs",
        "runtime/spinel/active_support_interval.rbs",
        "runtime/spinel/active_support_duration.rbs",
    ],
    include_env: Some(("SPINEL_PG_DIR", "a spinel-pg checkout")),
    lane_file: Some("tests/db_contract/db_pg_lane.rb"),
    lane_checks: 104,
}];

/// The lanes one test runs.
pub fn lanes(runtime: Runtime, backend: Backend) -> impl Iterator<Item = &'static Lane> {
    LANES
        .iter()
        .filter(move |l| l.runtime == runtime && l.backend == backend)
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

struct ScratchDir(PathBuf);
impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy(from: &Path, to: &Path) {
    std::fs::copy(from, to)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", from.display(), to.display()));
}

fn run(command: &mut Command) -> String {
    let out = command
        .output()
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{command:?}: {}\n=== stdout ===\n{stdout}\n=== stderr ===\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    print!("{stdout}");
    stdout
}

/// The count a suite reported, or a failure naming the missing line.
fn reported(stdout: &str, suite: &str) -> usize {
    let prefix = format!("{suite}: ");
    stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix(&prefix)?
                .strip_suffix(" checks passed")?
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("no `{suite}: N checks passed` line\n{stdout}"))
}

fn required_env(name: &str, what: &str, lane: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => v,
        _ => panic!(
            "{name} must name {what} for the {lane} lane; it fails rather than skips without it"
        ),
    }
}

/// Runs one lane. A PostgreSQL lane needs DATABASE_URL and creates one
/// schema named after the process, dropped when it finishes.
pub fn run_lane(lane: &Lane) {
    let test = format!("db_contract_{}", lane.name);
    let pg = match lane.backend {
        Backend::Sqlite => None,
        Backend::Postgres => Some(required_env(
            "DATABASE_URL",
            "a PostgreSQL server",
            lane.name,
        )),
    };
    let include = lane
        .include_env
        .map(|(var, what)| required_env(var, what, lane.name));
    let base = option_env!("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("roundhouse-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sig")).unwrap();
    let _cleanup = ScratchDir(dir.clone());

    let suite = root().join("tests/db_contract");
    copy(&root().join(lane.shim), &dir.join("db.rb"));
    for file in lane.requires {
        let path = Path::new(file);
        copy(&root().join(path), &dir.join(path.file_name().unwrap()));
    }
    for file in lane.rbs {
        let path = Path::new(file);
        copy(
            &root().join(path),
            &dir.join("sig").join(path.file_name().unwrap()),
        );
    }
    let prelude = match lane.runtime {
        Runtime::Cruby => "cruby_prelude.rb",
        Runtime::Spinel => "spinel_prelude.rb",
    };
    copy(&suite.join(prelude), &dir.join("prelude.rb"));
    for file in ["support.rb", "contract_cases.rb", "postgres_cases.rb"] {
        copy(&suite.join(file), &dir.join(file));
    }
    let main = match lane.backend {
        Backend::Sqlite => "sqlite_main.rb",
        Backend::Postgres => {
            let lane_file = lane
                .lane_file
                .unwrap_or_else(|| panic!("the PostgreSQL lane {} names no lane file", lane.name));
            copy(&root().join(lane_file), &dir.join("lane.rb"));
            "postgres_main.rb"
        }
    };
    copy(&suite.join(main), &dir.join("main.rb"));
    println!("{} lane tree: {}", lane.name, dir.display());

    // Bounded: a lease that leaks, or a lock a broken case leaves
    // behind, must fail the lane rather than hang the job.
    let mut program = Command::new("timeout");
    program.arg("300");
    match lane.runtime {
        Runtime::Cruby => {
            program
                .arg("ruby")
                .arg("main.rb")
                .env("ROUNDHOUSE_ROOT", root());
        }
        Runtime::Spinel => {
            let compiler = std::env::var("SPINEL").unwrap_or_else(|_| "spinel".into());
            let mut compile = Command::new(compiler);
            if let Some(include) = &include {
                compile.args(["-I", include, "--strict-require"]);
            }
            if !lane.rbs.is_empty() {
                compile.args(["--rbs", "."]);
            }
            run(compile
                .args(["main.rb", "-o", "contract_gate"])
                .current_dir(&dir));
            program.arg(dir.join("contract_gate"));
        }
    }
    if lane.backend == Backend::Sqlite {
        program.arg(dir.join("contract.sqlite3"));
    }
    if let Some(url) = &pg {
        program.env("DATABASE_URL", url).env(
            "SPINEL_PG_SCHEMA",
            format!("rh_spinel_pg_db_{}", std::process::id()),
        );
    }
    let stdout = run(program
        .env_remove("DATABASE_POOL_SIZE")
        .env_remove("RH_SQL_TRACE")
        .current_dir(&dir));

    assert_eq!(
        reported(&stdout, "contract"),
        CONTRACT_CHECKS,
        "the {} lane ran a different number of contract checks than contract_cases.rb makes",
        lane.name
    );
    if lane.backend == Backend::Postgres {
        assert_eq!(
            reported(&stdout, "postgres"),
            POSTGRES_CHECKS,
            "the {} lane ran a different number of PostgreSQL checks than postgres_cases.rb makes",
            lane.name
        );
        assert_eq!(
            reported(&stdout, lane.name),
            lane.lane_checks,
            "the {} lane's own checks",
            lane.name
        );
    }
}
