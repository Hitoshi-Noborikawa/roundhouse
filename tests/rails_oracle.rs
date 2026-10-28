//! `tests/support/rails_oracle.rb` picks the Rails a native control loads:
//! the installed release the suite pins by default, another release, or a
//! Rails checkout, from `RH_RAILS_ORACLE`. Each test runs the helper under
//! plain `ruby` the way a control does.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ORACLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/rails_oracle.rb");

fn oracle_ruby(oracle: Option<&str>, script: &str) -> Output {
    let mut command = Command::new("ruby");
    command.args(["-e", &format!("require {ORACLE:?}\n{script}")]);
    match oracle {
        Some(value) => command.env("RH_RAILS_ORACLE", value),
        None => command.env_remove("RH_RAILS_ORACLE"),
    };
    command.output().expect("ruby")
}

fn stdout_of(output: &Output) -> String {
    assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Whether the default release's activesupport is installed; a run without
/// it says so and passes, as the suite's other native controls do.
fn release_installed() -> bool {
    let probe = oracle_ruby(None, r#"exit(RailsOracle.available?("activesupport") ? 0 : 1)"#);
    if !probe.status.success() {
        eprintln!("skipping: the default release of activesupport is not installed");
    }
    probe.status.success()
}

/// A throwaway Rails checkout holding one component, `activesupport`, at
/// `version`, whose library answers where it was loaded from.
fn checkout(name: &str, version: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("rh-rails-oracle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let component = root.join("activesupport");
    std::fs::create_dir_all(component.join("lib/active_support")).unwrap();
    std::fs::write(root.join("RAILS_VERSION"), format!("{version}\n")).unwrap();
    std::fs::write(
        component.join("activesupport.gemspec"),
        format!(
            "Gem::Specification.new do |s|\n  s.name = \"activesupport\"\n  s.version = {version:?}\n  s.summary = \"checkout\"\n  s.authors = [\"oracle test\"]\n  s.files = []\n  s.require_paths = [\"lib\"]\nend\n"
        ),
    )
    .unwrap();
    std::fs::write(component.join("lib/active_support/oracle_probe.rb"), "ORACLE_PROBE = __FILE__\n").unwrap();
    root
}

fn remove(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_default_oracle_is_the_release_the_suite_pins() {
    if !release_installed() {
        return;
    }
    let output = oracle_ruby(
        None,
        r#"RailsOracle.activate("activesupport")
require "active_support"
puts [ActiveSupport.version.to_s, RailsOracle.label].join("|")"#,
    );
    assert_eq!(stdout_of(&output), "8.1.4|Rails 8.1.4 (installed gems)");
}

#[test]
fn another_gem_takes_a_requirement_not_one_exact_build() {
    if !release_installed() {
        return;
    }
    let output = oracle_ruby(
        None,
        r#"RailsOracle.activate("activesupport", minitest: ">= 5.0")
puts Gem.loaded_specs.fetch("minitest").version >= Gem::Version.new("5.0")"#,
    );
    assert_eq!(stdout_of(&output), "true");
}

#[test]
fn a_release_that_is_not_installed_is_reported_unavailable() {
    let output = oracle_ruby(Some("0.0.1"), r#"puts RailsOracle.available?("activesupport")"#);
    assert_eq!(stdout_of(&output), "false");
}

#[test]
fn a_checkout_supplies_the_components_even_beside_a_newer_release() {
    // A prerelease, as a Rails main checkout is: RubyGems would otherwise
    // take an installed release of the same component over it.
    let root = checkout("prerelease", "8.0.0.alpha");
    let output = oracle_ruby(
        Some(root.to_str().unwrap()),
        r#"puts RailsOracle.available?("activesupport")
RailsOracle.activate("activesupport")
require "active_support/oracle_probe"
puts Gem.loaded_specs.fetch("activesupport").version
puts ORACLE_PROBE.start_with?(File.expand_path(RailsOracle.source))
puts RailsOracle.label.start_with?("Rails 8.0.0.alpha (")"#,
    );
    remove(&root);
    assert_eq!(stdout_of(&output), "true\n8.0.0.alpha\ntrue\ntrue");
}

#[test]
fn a_component_the_checkout_lacks_is_refused_by_name() {
    let root = checkout("partial", "8.0.0");
    let output = oracle_ruby(Some(root.to_str().unwrap()), r#"RailsOracle.activate("activesupport", "activerecord")"#);
    remove(&root);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("activerecord is not in the Rails checkout at"), "{stderr}");
}

#[test]
fn a_missing_checkout_is_refused_naming_the_path() {
    let missing = std::env::temp_dir().join("rh-rails-oracle-no-such-checkout");
    let output = oracle_ruby(Some(missing.to_str().unwrap()), r#"RailsOracle.activate("activesupport")"#);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("no Rails checkout at {}", missing.display())), "{stderr}");
}
