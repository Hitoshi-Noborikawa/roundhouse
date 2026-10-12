//! A class-body `include` guarded by `begin … rescue NameError … end`.
//!
//! ```ruby
//! class Part < ApplicationRecord
//!   begin
//!     include(Chime)
//!   rescue NameError
//!   end
//! end
//! ```
//!
//! is the "include it when the module is there" idiom. The model body
//! walk only knew calls, so the `begin` was an unknown item that no
//! later pass read: the include never reached the class, and the
//! first call to one of the module's methods raised `NoMethodError`.
//! Now the guarded body is the class's own declarations when every
//! module it names is defined in the app, and nothing when one is not
//! (in Rails the rescue swallows that `NameError`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const SCHEMA: &str = r#"ActiveRecord::Schema.define(version: 1) do
  create_table "parts", force: :cascade do |t|
    t.string "name"
  end
end
"#;

const CHIME: &str = "module Chime\n  def ring(code)\n    \"rang #{code}\"\n  end\nend\n";

fn part(head: &str) -> String {
    format!(
        "class Part < ApplicationRecord\n{head}\n  def announce\n    ring(\"part\")\n  end\nend\n"
    )
}

/// The emitted `part` model files for a model body opening with `head`.
fn emitted_part(target: BuildTarget, head: &str) -> String {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(
        PathBuf::from("app/models/application_record.rb"),
        b"class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n".to_vec(),
    );
    tree.insert(PathBuf::from("app/models/concerns/chime.rb"), CHIME.as_bytes().to_vec());
    tree.insert(PathBuf::from("app/models/part.rb"), part(head).into_bytes());
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = target_files(&app, Path::new("fixtures/tiny-blog"), target).expect("emit");
    let mut parts: Vec<(String, String)> = files
        .into_iter()
        .filter(|(p, _)| p.ends_with("models/part.rb") || p.ends_with("models/part.rbs"))
        .collect();
    assert!(!parts.is_empty(), "no part model emitted");
    parts.sort();
    // Source-line markers name the model's own lines, which the block
    // above them shifts; the code is what is compared.
    parts
        .into_iter()
        .map(|(p, c)| {
            let code: Vec<&str> = c.lines().filter(|l| !l.starts_with("#<SPINEL_SOURCE>")).collect();
            format!("# {p}\n{}", code.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const PLAIN: &str = "  include Chime\n";
const GUARDED: &str = "  begin\n    include(Chime)\n  rescue NameError\n  end\n";
const GUARDED_LOAD: &str = "  begin\n    include Chime\n  rescue NameError, LoadError\n    nil\n  end\n";

#[test]
fn a_guarded_include_of_a_defined_module_is_the_include() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let plain = emitted_part(target, PLAIN);
        assert!(plain.contains("ring"), "{target:?}: the plain include reaches the model:\n{plain}");
        for guarded in [GUARDED, GUARDED_LOAD] {
            assert_eq!(emitted_part(target, guarded), plain, "{target:?}: {guarded}");
        }
    }
}

/// A module the app does not define: Rails' rescue swallows the
/// `NameError`, and the class is what it would be without the block.
#[test]
fn a_guarded_include_of_an_undefined_module_is_nothing() {
    let missing = "  begin\n    include(Bell)\n  rescue NameError\n  end\n";
    let partly = "  begin\n    include Chime\n    include Bell\n  rescue NameError\n  end\n";
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let bare = emitted_part(target, "");
        assert_eq!(emitted_part(target, missing), bare, "{target:?}");
        assert_eq!(emitted_part(target, partly), bare, "{target:?}");
    }
}
