//! An enum that takes its mapping from another class:
//!
//! ```ruby
//! class Widget < ApplicationRecord
//!   enum :status, Gadget.statuses
//!   enum :mode, Sprocket.vendors.keys.freeze
//!   enum :category, Gadget.kinds.merge(Sprocket.vendors)
//!   enum :vendor, Sprocket::VENDORS
//! end
//! ```
//!
//! `Gadget.statuses` is the class method Rails' `enum :status` defines on
//! `Gadget`: the label => value mapping that declaration built, as a
//! frozen HashWithIndifferentAccess. It is as static as the declaration,
//! so ingest folds it from the owner's own `enum` (literal, own constant,
//! or another owner's mapping in turn) when nothing else can answer that
//! method: no singleton definition of the same name, no reflection on the
//! owner, one declaration of the column, no load-order cycle. `.keys`
//! (labels, as Strings) and `.merge` (Hash#merge on the indifferent hash:
//! a repeated label keeps its first position and takes the later value)
//! derive from it the way Ruby does. Every answer below is Rails 8.1.4's.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::expr::Literal;
use roundhouse::ingest::{ingest_app_from_tree, survey};
use roundhouse::Symbol;

const SCHEMA: &str = "ActiveRecord::Schema.define do
  create_table \"gadgets\", force: :cascade do |t|
    t.integer \"kind\"
    t.integer \"status\"
  end
  create_table \"sprockets\", force: :cascade do |t|
    t.integer \"vendor\"
  end
  create_table \"widgets\", force: :cascade do |t|
    t.integer \"kind\"
    t.integer \"status\"
    t.integer \"role\"
    t.integer \"category\"
    t.integer \"mode\"
    t.integer \"vendor\"
  end
end
";

const GADGET: &str = "class Gadget < ApplicationRecord
  KINDS = %i[small medium large].freeze
  enum :kind, KINDS
  enum :status, %i[draft live archived].freeze
end
";

const SPROCKET: &str = "class Sprocket < ApplicationRecord
  VENDORS = { acme: 0, globex: 1, initech: 2, unknown: nil }.freeze
  public_constant :VENDORS
  enum :vendor, VENDORS
end
";

fn ingest(files: &[(&str, &str)]) -> (roundhouse::App, Vec<String>) {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(
        PathBuf::from("app/models/application_record.rb"),
        b"class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n".to_vec(),
    );
    for (path, source) in files {
        tree.insert(PathBuf::from(path), source.as_bytes().to_vec());
    }
    survey::activate();
    let app = ingest_app_from_tree(tree).expect("ingest");
    let gaps = survey::drain().into_iter().map(|g| g.to_string()).collect();
    (app, gaps)
}

fn mapping(app: &roundhouse::App, model: &str, column: &str) -> Option<Vec<(String, Literal)>> {
    let model = app.models.iter().find(|m| m.name.0.as_str() == model).unwrap_or_else(|| panic!("{model}"));
    model.enums.get(&Symbol::from(column)).cloned()
}

fn int(label: &str, value: i64) -> (String, Literal) {
    (label.to_string(), Literal::Int { value })
}

fn statuses() -> Vec<(String, Literal)> {
    vec![int("draft", 0), int("live", 1), int("archived", 2)]
}

fn vendors() -> Vec<(String, Literal)> {
    vec![int("acme", 0), int("globex", 1), int("initech", 2), ("unknown".to_string(), Literal::Nil)]
}

#[test]
fn another_models_enum_mapping_folds() {
    let (app, gaps) = ingest(&[
        ("app/models/gadget.rb", GADGET),
        ("app/models/sprocket.rb", SPROCKET),
        (
            "app/models/widget.rb",
            "class Widget < ApplicationRecord
  enum :kind, Gadget::KINDS, prefix: true
  enum :status, Gadget.statuses, prefix: true
  enum :role, Gadget::statuses, prefix: true
  enum :category, Gadget.kinds.merge(Sprocket.vendors), prefix: true
  enum :mode, Sprocket.vendors.keys.freeze, prefix: true
  enum :vendor, Sprocket.vendors, prefix: true
end
",
        ),
    ]);
    assert!(!gaps.iter().any(|g| g.contains("widget.rb")), "{gaps:?}");
    assert_eq!(mapping(&app, "Widget", "kind"), Some(vec![int("small", 0), int("medium", 1), int("large", 2)]));
    assert_eq!(mapping(&app, "Widget", "status"), Some(statuses()));
    assert_eq!(mapping(&app, "Widget", "role"), Some(statuses()));
    let mut categories = vec![int("small", 0), int("medium", 1), int("large", 2)];
    categories.extend(vendors());
    assert_eq!(mapping(&app, "Widget", "category"), Some(categories));
    // `.keys` is the labels, as Strings: an Array mapping, by position.
    assert_eq!(
        mapping(&app, "Widget", "mode"),
        Some(vec![int("acme", 0), int("globex", 1), int("initech", 2), int("unknown", 3)])
    );
    assert_eq!(mapping(&app, "Widget", "vendor"), Some(vendors()));
}

/// A Hash constant of another owner, read the way #1177 reads one for a
/// validation.
#[test]
fn another_owners_hash_constant_folds() {
    let (app, gaps) = ingest(&[
        ("app/models/sprocket.rb", SPROCKET),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\n  enum :vendor, Sprocket::VENDORS\nend\n"),
    ]);
    assert_eq!(mapping(&app, "Widget", "vendor"), Some(vendors()), "{gaps:?}");
}

/// `merge` on the indifferent hash: a label both sides name keeps its
/// first position and takes the later value; new labels go last.
#[test]
fn merge_keeps_the_first_position_and_the_later_value() {
    let (app, gaps) = ingest(&[
        ("app/models/gadget.rb", GADGET),
        ("app/models/sprocket.rb", SPROCKET),
        (
            "app/models/widget.rb",
            "class Widget < ApplicationRecord
  enum :category, Gadget.kinds.merge(Sprocket.vendors).merge(small: 7, extra: 9)
end
",
        ),
    ]);
    let mut want = vec![int("small", 7), int("medium", 1), int("large", 2)];
    want.extend(vendors());
    want.push(int("extra", 9));
    assert_eq!(mapping(&app, "Widget", "category"), Some(want), "{gaps:?}");
}

/// Reflection that cannot reach the enum's class method leaves it alone:
/// `Object.const_get` hands out a class, and an `instance_eval` block
/// that defines nothing defines no singleton method.
#[test]
fn reflection_that_defines_nothing_still_folds() {
    let (app, gaps) = ingest(&[
        ("app/models/gadget.rb", GADGET),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\n  enum :status, Gadget.statuses\nend\n"),
        (
            "lib/loader.rb",
            "module Loader\n  def self.included(mod)\n    mod.instance_eval do\n      before_action(:load) if mod.respond_to?(:before_action)\n    end\n  end\n\n  def self.find(type) = Object.const_get(type.classify)\nend\n",
        ),
    ]);
    assert_eq!(mapping(&app, "Widget", "status"), Some(statuses()), "{gaps:?}");
}

/// The owner's mapping may itself come from another owner, and file
/// order does not matter.
#[test]
fn a_chain_of_owners_folds() {
    let (app, gaps) = ingest(&[
        ("app/models/doohickey.rb", "class Doohickey < ApplicationRecord\n  self.table_name = \"widgets\"\n  enum :status, Widget.statuses\nend\n"),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\n  enum :status, Gadget.statuses\nend\n"),
        ("app/models/gadget.rb", GADGET),
    ]);
    assert_eq!(mapping(&app, "Doohickey", "status"), Some(statuses()), "{gaps:?}");
}

/// Each case where Ruby may answer something else (or raise) stays the
/// reported gap rather than a guess.
#[test]
fn a_mapping_ruby_may_answer_differently_stays_a_gap() {
    let cases: &[(&str, &[(&str, &str)])] = &[
        // The owner defines the class method itself.
        ("own singleton", &[(
            "app/models/gadget.rb",
            "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\n  def self.statuses\n    { \"x\" => 1 }\n  end\nend\n",
        )]),
        ("class << self", &[(
            "app/models/gadget.rb",
            "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\n  class << self\n    def statuses = {}\n  end\nend\n",
        )]),
        ("def Owner.m elsewhere", &[
            ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n"),
            ("lib/patch.rb", "def Gadget.statuses = {}\n"),
        ]),
        ("reflection", &[
            ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n"),
            ("lib/patch.rb", "Gadget.class_eval { }\n"),
        ]),
        ("instance_eval def", &[
            ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n"),
            ("lib/patch.rb", "module Patch\n  def self.apply(klass)\n    klass.instance_eval do\n      def statuses = {}\n    end\n  end\nend\n"),
        ]),
        ("const_get define_singleton_method", &[
            ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n"),
            ("lib/patch.rb", "Object.const_get(\"Gadget\").define_singleton_method(:statuses) { {} }\n"),
        ]),
        ("singleton_class on a value", &[
            ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n"),
            ("lib/patch.rb", "def patch(klass) = klass.singleton_class.prepend(Module.new)\n"),
        ]),
        ("define_singleton_method", &[(
            "app/models/gadget.rb",
            "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\n  define_singleton_method(:statuses) { {} }\nend\n",
        )]),
        // No `enum :status` of Gadget's own (a concern's, or none):
        // nothing here says what `statuses` answers.
        ("no declaration", &[("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  include HasStatus\nend\n")]),
        // Declared twice, in two bodies.
        ("reopened", &[
            ("app/models/gadget.rb", "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n"),
            ("lib/gadget_ext.rb", "class Gadget\n  enum :status, %i[other]\nend\n"),
        ]),
        // Hash#merge with an Array raises TypeError.
        ("merge array", &[(
            "app/models/gadget.rb",
            "class Gadget < ApplicationRecord\n  enum :status, %i[draft live]\nend\n",
        )]),
        // A plain Hash's merge keeps a Symbol and a String key apart.
        ("plain hash merge", &[("app/models/sprocket.rb", SPROCKET), ("app/models/gadget.rb", GADGET)]),
        // A mapping that comes back to the model being loaded.
        ("cycle", &[(
            "app/models/gadget.rb",
            "class Gadget < ApplicationRecord\n  enum :status, Widget.statuses\nend\n",
        )]),
    ];
    for (name, files) in cases {
        let widget = match *name {
            "merge array" => "class Widget < ApplicationRecord\n  enum :status, Gadget.statuses.merge(%i[x])\nend\n",
            "plain hash merge" => "class Widget < ApplicationRecord\n  enum :status, Sprocket::VENDORS.merge(Gadget.statuses)\nend\n",
            _ => "class Widget < ApplicationRecord\n  enum :status, Gadget.statuses\nend\n",
        };
        let mut all = files.to_vec();
        all.push(("app/models/widget.rb", widget));
        let (app, gaps) = ingest(&all);
        assert_eq!(mapping(&app, "Widget", "status"), None, "{name}");
        assert!(gaps.iter().any(|g| g.contains("enum :status mapping")), "{name}: {gaps:?}");
    }
}

/// A model reading its own enum's mapping for a second one: Ruby only
/// has the method once the first declaration ran, so this stays a gap
/// rather than depending on statement order.
#[test]
fn the_models_own_mapping_stays_a_gap() {
    let (app, gaps) = ingest(&[(
        "app/models/widget.rb",
        "class Widget < ApplicationRecord\n  enum :status, %i[draft live]\n  enum :role, Widget.statuses\nend\n",
    )]);
    assert_eq!(mapping(&app, "Widget", "role"), None);
    assert!(gaps.iter().any(|g| g.contains("enum :role mapping")), "{gaps:?}");
}
