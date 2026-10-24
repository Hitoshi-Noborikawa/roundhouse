//! `alias new old` where `old` is a method every Object answers (`send`,
//! `to_s`, `class`, …) rather than one the class defines. The
//! ActiveModel::Serializers convention is `alias
//! read_attribute_for_serialization send`. Ruby captures the method at the
//! alias, so each alias answers as the builtin does (Ruby 4.0.7).
//!
//! The alias was refused ("alias names a method this body has not
//! defined"), which left the class an unsupported constant. On the ruby
//! family it now delegates to the builtin. A class that defines the
//! builtin's name itself stays the gap, because the delegation would reach
//! that definition where Ruby's alias does not. The strict targets refuse it
//! and name the builtin.

use roundhouse::project::BuildTarget;

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const PRESENTER: &str = r#"class NamePresenter
  attr_reader :name

  def initialize(name)
    @name = name
  end

  alias read_attribute_for_serialization send
  alias call_public public_send
  alias raw_send __send__
  alias label to_s
  alias describe inspect
  alias kind class
  alias me itself
  alias oid object_id
  alias digest hash
  alias same? equal?
  alias locked? frozen?
  alias a? is_a?
  alias kind_of_thing? kind_of?
  alias exactly? instance_of?
  alias answers? respond_to?
  alias none? nil?
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/presenters/name_presenter.rb", PRESENTER)
}

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
x = NamePresenter.new("Ada")
check("send", x.read_attribute_for_serialization(:name), "Ada")
check("send with a String", x.read_attribute_for_serialization("name"), "Ada")
check("public_send", x.call_public(:name), "Ada")
check("__send__", x.raw_send(:name), "Ada")
check("to_s", x.label, x.to_s)
check("inspect", x.describe.start_with?("#<NamePresenter"), true)
check("class", x.kind, NamePresenter)
check("itself", x.me.equal?(x), true)
check("object_id", x.oid, x.object_id)
check("hash", x.digest, x.hash)
check("equal? self", x.same?(x), true)
check("equal? other", x.same?(NamePresenter.new("Ada")), false)
check("frozen?", x.locked?, false)
check("is_a?", x.a?(NamePresenter), true)
check("kind_of?", x.kind_of_thing?(Object), true)
check("instance_of?", x.exactly?(Object), false)
check("respond_to? yes", x.answers?(:name), true)
check("respond_to? no", x.answers?(:nope), false)
check("nil?", x.none?, false)
check("respond_to? include_all", x.answers?(:initialize, true), true)
arity = begin
  x.label(1)
  "no error"
rescue ArgumentError
  "ArgumentError"
end
check("to_s keeps its arity", arity, "ArgumentError")
puts "builtin aliases passed"
"##;

#[test]
fn an_alias_of_a_builtin_answers_as_the_builtin_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("builtin aliases passed"), "{}", run.stdout);
}

/// A class that defines the builtin's name keeps the gap: Ruby's alias
/// captured `Object#to_s`, and a delegating method would reach the class's
/// own `to_s` instead.
#[test]
fn a_builtin_the_class_redefines_stays_the_gap() {
    use std::collections::HashMap;
    use std::path::PathBuf;
    let tree: HashMap<PathBuf, Vec<u8>> = [
        (
            "app/presenters/label_presenter.rb",
            "class LabelPresenter\n  alias label to_s\n\n  def to_s\n    \"mine\"\n  end\nend\n",
        ),
        ("app/presenters/name_presenter.rb", PRESENTER),
        ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
    ]
    .into_iter()
    .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
    .collect();
    roundhouse::ingest::survey::activate();
    let app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let gaps: Vec<String> = roundhouse::ingest::survey::drain().into_iter().map(|g| g.to_string()).collect();
    assert!(gaps.iter().any(|g| g.contains("label_presenter.rb") && g.contains("alias")), "{gaps:#?}");
    assert!(!app.library_classes.iter().any(|c| c.name.0.as_str() == "LabelPresenter"));
    assert!(app.library_classes.iter().any(|c| c.name.0.as_str() == "NamePresenter"), "{gaps:#?}");
}

/// The strict targets have no `send` to delegate to: refused where it is
/// written, naming the builtin.
#[test]
fn a_strict_target_refuses_and_names_the_builtin() {
    let (_emitted, errors) = app().emit(BuildTarget::Rust);
    assert!(errors.iter().any(|e| e.contains("`send`") && e.contains("read_attribute_for_serialization")), "{errors:#?}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_alias_of_a_builtin_answers_as_the_builtin_on_spinel() {
    let run = app().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("builtin aliases passed"), "{}", run.stdout);
}
