//! PostgreSQL on the Ruby target: an app whose `config/database.yml`
//! names postgresql for the test environment's primary gets SQL lowered
//! for PostgreSQL and a tree that runs it on the pg gem
//! (`project::apply_postgresql_tree`): runtime/spinel/db_pg.rb as its Db,
//! pg_driver_cruby.rb as its driver, `gem "pg"`, and its database the
//! server `DATABASE_URL` names, one schema per test process.
//!
//! The emitted-tree checks need no server. The runs do: they read
//! ROUNDHOUSE_PG_TEST_URL through tests/support/pg_test_url.rs, skipping
//! without it unless ROUNDHOUSE_PG_TEST_REQUIRED is set, and need the pg
//! gem.
//!
//!   ROUNDHOUSE_PG_TEST_URL=postgres://postgres@127.0.0.1:5432/postgres \
//!   cargo test --test postgresql_ruby_target

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/pg_test_url.rs"]
mod pg_test_url;

use roundhouse::project::BuildTarget;

const POSTGRESQL_DATABASE_YML: &str = "default: &default\n  adapter: postgresql\n  encoding: unicode\n  max_connections: <%= ENV.fetch(\"RAILS_MAX_THREADS\") { 5 } %>\n\ndevelopment:\n  <<: *default\n  database: blog_development\n\ntest:\n  <<: *default\n  database: blog_test\n";

fn postgresql_blog() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("config/database.yml", POSTGRESQL_DATABASE_YML)
}

fn read(emitted: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(emitted.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

#[test]
fn a_postgresql_primary_ships_the_pg_tree() {
    let (emitted, errors) = postgresql_blog().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let db = read(&emitted, "runtime/db.rb");
    assert!(db.contains("Primitive Db surface over PostgreSQL"), "runtime/db.rb is not db_pg.rb");
    assert!(read(&emitted, "runtime/pg_driver.rb").contains("dbname: database"), "runtime/pg_driver.rb is not the pg gem's");
    assert!(!emitted.join("runtime/db_pg.rb").exists() && !emitted.join("runtime/pg_driver_cruby.rb").exists());
    let gemfile = read(&emitted, "Gemfile");
    assert!(gemfile.contains("gem \"pg\"") && !gemfile.contains("gem \"sqlite3\""), "{gemfile}");
    assert!(!emitted.join("Gemfile.lock").exists(), "the committed lock is SQLite's");
    assert!(read(&emitted, "main.rb").contains("ENV.fetch(\"DATABASE_URL\", \"\")"));
    assert!(read(&emitted, "test/test_helper.rb").contains("search_path"));

    // SQL lowered for PostgreSQL: its DDL, and a truncate that resets
    // the table's identity in the statement itself.
    let schema = read(&emitted, "config/schema.rb");
    assert!(schema.contains("\\\"id\\\" bigserial PRIMARY KEY"), "{schema}");
    let article = read(&emitted, "app/models/article.rb");
    assert!(article.contains("TRUNCATE TABLE \\\"articles\\\" RESTART IDENTITY CASCADE"), "{article}");
    assert!(!article.contains("sqlite_sequence"), "{article}");
}

#[test]
fn a_sqlite_primary_keeps_the_sqlite_tree() {
    let (emitted, errors) = emit_and_run::real_blog().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(read(&emitted, "runtime/db.rb").contains("require \"sqlite3\""));
    assert!(read(&emitted, "Gemfile").contains("gem \"sqlite3\""));
    assert!(read(&emitted, "app/models/article.rb").contains("sqlite_sequence"));
}

/// The blog's own Rails tests, emitted for PostgreSQL, run on a server.
#[test]
fn the_blogs_tests_run_on_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_ruby_target") else { return };
    for test in [
        "test/models/article_test.rb",
        "test/models/comment_test.rb",
        "test/controllers/articles_controller_test.rb",
        "test/controllers/comments_controller_test.rb",
    ] {
        postgresql_blog().run_test_env(test, &[("DATABASE_URL", db.url.as_str())]).assert_passes();
    }
}

/// The server boot path (`Main.configure_default_adapter!`), with the
/// tables in a schema of the test's own. A boolean literal in a lowered
/// `where` is PostgreSQL's `TRUE`, which a boolean column compares with;
/// SQLite's `1` is an error there.
#[test]
fn a_record_round_trips_through_postgresql() {
    let Some(db) = pg_test_url::pg_test_db("postgresql_ruby_target_boot") else { return };
    // Boot creates the tables, so the schema it creates them in comes
    // first; it is dropped whatever the run does.
    let on_server = |sql: String| {
        let output = std::process::Command::new("ruby")
            .args(["-rpg", "-e", "PG.connect(ARGV[0]).exec(ARGV[1])", &db.url, &sql])
            .output()
            .expect("ruby with the pg gem");
        assert!(output.status.success(), "{sql}: {}", String::from_utf8_lossy(&output.stderr));
    };
    on_server(format!("CREATE SCHEMA {}", db.schema));
    let options = format!("-c search_path={}", db.schema);
    let run = postgresql_blog()
        .edit("db/schema.rb", "    t.text \"body\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"", "    t.text \"body\"\n    t.boolean \"published\", default: false, null: false\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\"")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  def self.published_count\n    where(published: true).count\n  end\n\n")
        .run_ruby_env(
            r#"article = Article.create!(title: "On PostgreSQL", body: "A body long enough.")
found = Article.find(article.id)
raise "title #{found.title.inspect}" unless found.title == "On PostgreSQL"
raise "count #{Article.count}" unless Article.count == 1
raise "published #{Article.published_count}" unless Article.published_count == 0
article.update!(published: true)
raise "published #{Article.published_count}" unless Article.published_count == 1
article.destroy
raise "left #{Article.count}" unless Article.count == 0
print "round trip"
"#,
            &[("DATABASE_URL", db.url.as_str()), ("PGOPTIONS", options.as_str())],
    );
    on_server(format!("DROP SCHEMA {} CASCADE", db.schema));
    run.assert_passes();
    assert_eq!(run.stdout, "round trip");
}

#[test]
fn sqlite_functions_on_a_postgresql_primary_are_refused() {
    let (_emitted, errors) = postgresql_blog()
        .write("config/initializers/sqlite.rb", "conn.create_function(\"regexp\", 2) do |fn, pattern, expr|\n  fn.result = 1\nend\n")
        .emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("SQLite functions (regexp)") && e.contains("postgresql")), "{errors:#?}");
}
