//! A keyword call through a NILABLE receiver (`Gadget | Nil`, from
//! `Array#first`, `find_by`, an `[]` read) falls through the
//! typed-receiver respell at
//! `lower::helper_kwargs::constructor::rewrite_instance_call` (#800's
//! remainder past #814's `Ty::Class`-only fix).
//!
//! `def deliver(connection: nil)` is flattened to `def deliver(connection
//! = nil)` at ingest (every call site must move its keywords into
//! position). #814 moved the call for a receiver typed exactly
//! `Ty::Class`; a nilable union fell through with no rewrite and no
//! diagnostic, and Ruby bound the whole `{connection: "REAL"}` Hash to
//! the flattened `connection` slot instead of the string.
//!
//! Fix: drop `Nil` from the union before giving up. If one class
//! remains, or several remain that agree on this method's flattened
//! slot shape, respell exactly as for a plain `Ty::Class` receiver —
//! calling on the dropped `nil` branch raises `NoMethodError` in Ruby
//! either way, so narrowing it away never changes that call's behavior.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use std::sync::atomic::{AtomicUsize, Ordering};

static RUN: AtomicUsize = AtomicUsize::new(0);

fn native(files: &[(&str, &str)], script: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "roundhouse-nilable-kwargs-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut requires = String::new();
    for (name, source) in files {
        let path = dir.join(name);
        std::fs::write(&path, source).unwrap();
        requires.push_str(&format!("require {:?}\n", path.display().to_string()));
    }
    let output = std::process::Command::new("ruby")
        .arg("-e")
        .arg(format!("{requires}{script}"))
        .output()
        .expect("native Ruby control");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The write-up's reducer, verbatim: `[Gadget.new].first` types
/// `Gadget | Nil`, not a plain `Ty::Class`.
const SINGLE_CLASS_SOURCE: &str = r##"module Nilable
  class Gadget
    def deliver(connection: nil)
      connection
    end
  end

  class Courier
    def self.send_first
      [Gadget.new].first.deliver(connection: "REAL")
    end
  end
end
"##;

#[test]
fn a_single_class_nilable_receiver_binds_by_name() {
    let script = "p Nilable::Courier.send_first\n";
    let expected = "\"REAL\"\n";
    assert_eq!(
        native(&[("single_class.rb", SINGLE_CLASS_SOURCE)], script),
        expected,
        "native control"
    );

    let run = emit_and_run::real_blog()
        .write("app/lib/nilable/single_class.rb", SINGLE_CLASS_SOURCE)
        .run_ruby(&format!(
            "require_relative \"app/models/nilable/gadget\"\nrequire_relative \"app/models/nilable/courier\"\n{script}"
        ));
    run.assert_passes();
    assert_eq!(run.stdout, expected, "emitted vs native");

    let source =
        std::fs::read_to_string(run.emitted.join("app/models/nilable/courier.rb")).unwrap();
    assert!(
        source.contains("deliver(\"REAL\")"),
        "the keyword should move into the flattened `connection` slot, not stay a Hash:\n{source}"
    );
}

/// Two classes behind the union — `use_widget ? Widget.new : Gadget.new`
/// types `Widget | Gadget` — that agree on `deliver`'s flattened slot
/// shape. Whichever branch runs, the keyword lands in the same
/// position, so the call is rewritten against either.
const TWO_CLASS_SOURCE: &str = r##"module Nilable
  class Widget
    def deliver(connection: nil)
      connection
    end
  end

  class Gadget
    def deliver(connection: nil)
      connection
    end
  end

  class Courier
    def self.send_either(use_widget)
      (use_widget ? Widget.new : Gadget.new).deliver(connection: "REAL")
    end
  end
end
"##;

#[test]
fn a_two_class_union_with_matching_slot_shapes_binds_by_name() {
    let script = "p [Nilable::Courier.send_either(true), Nilable::Courier.send_either(false)]\n";
    let expected = "[\"REAL\", \"REAL\"]\n";
    assert_eq!(
        native(&[("two_class.rb", TWO_CLASS_SOURCE)], script),
        expected,
        "native control"
    );

    let run = emit_and_run::real_blog()
        .write("app/lib/nilable/two_class.rb", TWO_CLASS_SOURCE)
        .run_ruby(&format!(
            "require_relative \"app/models/nilable/widget\"\nrequire_relative \"app/models/nilable/gadget\"\nrequire_relative \"app/models/nilable/courier\"\n{script}"
        ));
    run.assert_passes();
    assert_eq!(run.stdout, expected, "emitted vs native");

    let source =
        std::fs::read_to_string(run.emitted.join("app/models/nilable/courier.rb")).unwrap();
    assert!(
        source.contains("deliver(\"REAL\")"),
        "both branches share a slot shape, so the call should move into it rather than stay a Hash:\n{source}"
    );
}
