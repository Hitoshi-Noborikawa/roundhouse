//! Bang find-or-create accepts literal and variable conditions.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\nend\n",
        )
        .write(
            "db/schema.rb",
            include_str!("find_or_create_bang_records/schema.rb"),
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n",
        )
        .write(
            "app/models/user.rb",
            "class User < ApplicationRecord\nend\n",
        )
        .write(
            "app/models/article.rb",
            "class Article < ApplicationRecord\nend\n",
        )
        .write(
            "app/models/follow.rb",
            include_str!("find_or_create_bang_records/follow.rb"),
        )
        .write(
            "app/models/favorite.rb",
            include_str!("find_or_create_bang_records/favorite.rb"),
        )
}

#[test]
fn pure_read_conditions_build_or_find_records_in_single_statement_branches() {
    let run = app().run_ruby(include_str!("find_or_create_bang_records/probe.rb"));
    run.assert_passes();
    assert!(
        run.stdout.contains("bang find-or-create passed"),
        "{}",
        run.stdout
    );
}

#[test]
fn record_conditions_and_validation_failures_match_rails() {
    let oracle = std::process::Command::new("ruby")
        .arg("tests/find_or_create_bang_records/oracle.rb")
        .output()
        .expect("run Rails oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let run = app().run_ruby(include_str!("find_or_create_bang_records/probe.rb"));
    run.assert_passes();
    assert_eq!(run.stdout, String::from_utf8_lossy(&oracle.stdout));
}

#[test]
fn method_call_conditions_keep_the_unsupported_boundary() {
    let run = app()
        .write(
            "app/models/follow.rb",
            r#"
class Follow < ApplicationRecord
  belongs_to :follower, class_name: "User"
  belongs_to :followed, class_name: "User"
  def self.effectful
    Follow.find_or_create_by!(follower: User.first, followed: User.first)
  end
end
"#,
        )
        .run_ruby("");
    assert!(
        run.errors
            .iter()
            .any(|error| error.contains("scoped find-or-create cannot be lowered safely")),
        "method calls would be evaluated for both lookup and construction: {:?}",
        run.errors
    );
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn bang_record_conditions_run_natively() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{}",
        include_str!("find_or_create_bang_records/probe.rb"),
    );
    app().run_spinel_with_rbs(&script).assert_passes();
}
