//! Mailer `before_action` / `after_action` / `around_action`. actionmailer
//! 8.1.4 (oracles `callbacks.rb` and `test_app/oracle.rb`, which runs these
//! same classes and script): one chain in declaration order, parent first.
//! Befores run in order, an around wraps the rest of the chain, and afters
//! run in reverse. `only:` / `except:` pick actions, `if:` takes a lambda,
//! and `with_options` merges into its calls. An after callback sees the
//! mail as `message`, and its `headers` reach the delivered mail.
//!
//! Each callback was reported and dropped, so the action ran without them.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const LOG: &str = "class CallbackLog\n  def self.entries\n    @entries ||= []\n  end\n\n  def self.add(entry)\n    entries << entry\n  end\n\n  def self.clear\n    @entries = []\n  end\nend\n";
const APPLICATION_MAILER: &str = "class ApplicationMailer < ActionMailer::Base\n  default from: \"from@example.com\"\n  layout \"mailer\"\n  before_action { CallbackLog.add(\"base_before\") }\n  after_action :stamp\n\n  def stamp\n    CallbackLog.add(\"stamp\")\n    headers(\"X-Stamp\" => \"yes\")\n  end\nend\n";
// App code that calls `with`, as a parameterized mailer's caller does.
const NOTICES: &str = "class Notices\n  def self.with(params)\n    NoticeMailer.with(params)\n  end\nend\n";

const NOTICE_MAILER: &str = "class NoticeMailer < ApplicationMailer\n  before_action :load_params\n  before_action :only_hello, only: :hello\n  before_action :not_hello, except: [:hello]\n  before_action :when_flag, if: -> { @flag }\n  with_options only: %i[bye] do\n    after_action :bye_after\n  end\n  after_action :after_one\n  after_action :after_two\n  around_action :wrap\n\n  def hello\n    CallbackLog.add(\"action\")\n    mail(to: @to, subject: \"Hello \" + action_name, body: \"hi\")\n  end\n\n  def bye\n    CallbackLog.add(\"action\")\n    mail(to: @to, subject: \"Bye\", body: \"bye\")\n  end\n\n  private\n\n  def load_params\n    CallbackLog.add(\"load_params\")\n    @to = params[:to] || \"someone@example.com\"\n    @flag = params[:flag]\n    throw :abort if params[:abort]\n  end\n\n  def only_hello\n    CallbackLog.add(\"only_hello\")\n  end\n\n  def not_hello\n    CallbackLog.add(\"not_hello\")\n  end\n\n  def when_flag\n    CallbackLog.add(\"when_flag\")\n  end\n\n  def bye_after\n    CallbackLog.add(\"bye_after\")\n  end\n\n  def after_one\n    CallbackLog.add(\"after_one\")\n    headers[\"X-One\"] = message.subject\n    headers[\"X-Seen-To\"] = message[\"To\"].value\n  end\n\n  def after_two\n    CallbackLog.add(\"after_two\")\n  end\n\n  def wrap\n    CallbackLog.add(\"around_in\")\n    yield\n    CallbackLog.add(\"around_out\")\n  end\nend\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
CallbackLog.clear
m = NoticeMailer.hello.deliver_now
check("the chain, in Rails' order", CallbackLog.entries, ["base_before", "load_params", "only_hello", "around_in", "action", "around_out", "after_two", "after_one", "stamp"])
check("action_name", m.subject, "Hello hello")
check("an after callback's headers reach the mail", [m["X-One"].value, m["X-Stamp"].value], ["Hello hello", "yes"])
check("an after callback reads the headers mail set", m["X-Seen-To"].value, "someone@example.com")
CallbackLog.clear
NoticeMailer.bye.deliver_now
check("except:, and with_options only:", CallbackLog.entries, ["base_before", "load_params", "not_hello", "around_in", "action", "around_out", "after_two", "after_one", "bye_after", "stamp"])
CallbackLog.clear
p = Notices.with(to: "p@example.com", flag: true).bye.deliver_now
check("params in a before_action, and if: a lambda", [p.to, CallbackLog.entries.include?("when_flag")], [["p@example.com"], true])
check("delivered", ActionMailer::Base.deliveries.size >= 3, true)
puts "mailer callbacks passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/callback_log.rb", LOG)
        .write("app/mailers/application_mailer.rb", APPLICATION_MAILER)
        .write("app/mailers/notice_mailer.rb", NOTICE_MAILER)
        .write("app/models/notices.rb", NOTICES)
}

#[test]
fn a_mailer_runs_its_callback_chain_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("mailer callbacks passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_mailer_runs_its_callback_chain_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("mailer callbacks passed"), "{}", run.stdout);
}

// `throw :abort` in a callback isn't caught (oracle: UncaughtThrowError).
#[test]
fn throw_abort_stays_uncaught_on_ruby() {
    let script = r##"
begin
  Notices.with(abort: true).hello.deliver_now
  puts "not raised"
rescue UncaughtThrowError => e
  puts "throw :abort: " + e.class.name
end
"##;
    let run = app().run_ruby(script);
    run.assert_passes();
    assert!(run.stdout.contains("throw :abort: UncaughtThrowError"), "{}", run.stdout);
}

/// The warnings ingest and `target`'s lowering give for the blog with
/// `files` written over it.
fn lowered_warnings(files: &[(&str, &str)], target: BuildTarget) -> Vec<String> {
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("roundhouse-mailer-callbacks-{}-{run}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("app");
    assert!(std::process::Command::new("cp").args(["-a"]).arg(roundhouse::fixtures::real_blog()).arg(&source).status().unwrap().success());
    for (path, text) in files {
        let full = source.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    let (app, ingest) = roundhouse::ingest::prism::scope(|| roundhouse::ingest::ingest_app(&source));
    let mut app = app.expect("ingest");
    let lowered = roundhouse::session::analyze_and_lower_for(&mut app, target);
    std::fs::remove_dir_all(&root).ok();
    ingest.into_iter().chain(lowered).map(|d| d.message).filter(|m| m.contains("mailer")).collect()
}

fn app_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("app/models/callback_log.rb", LOG),
        ("app/mailers/application_mailer.rb", APPLICATION_MAILER),
        ("app/mailers/notice_mailer.rb", NOTICE_MAILER),
        ("app/models/notices.rb", NOTICES),
    ]
}

// A strict target has no chain: each callback call is reported, where
// its emitter dropped it without a word.
#[test]
fn a_strict_target_reports_each_mailer_callback() {
    let warnings = lowered_warnings(&app_files(), BuildTarget::Typescript);
    for construct in ["mailer before_action", "mailer after_action", "mailer around_action", "mailer with_options"] {
        assert!(
            warnings.iter().any(|w| w.contains(construct) && w.contains("mailer callback not lowered")),
            "{construct}: {warnings:#?}"
        );
    }
    assert_eq!(warnings.iter().filter(|w| w.contains("mailer callback not lowered")).count(), 10, "{warnings:#?}");
}

#[test]
fn the_ruby_family_reports_no_mailer_callback() {
    let warnings = lowered_warnings(&app_files(), BuildTarget::Ruby);
    assert!(warnings.iter().all(|w| !w.contains("mailer callback not lowered")), "{warnings:#?}");
}

// A method named as the generated chain would name one is the app's own:
// the chain isn't generated, and the callbacks are reported.
#[test]
fn a_method_the_chain_would_name_keeps_the_callbacks_reported() {
    let mut files = app_files();
    let clashing = NOTICE_MAILER.replacen("  private\n", "  private\n\n  def __mailer_action_hello\n    nil\n  end\n", 1);
    let clashing: &'static str = Box::leak(clashing.into_boxed_str());
    files[2] = ("app/mailers/notice_mailer.rb", clashing);
    let warnings = lowered_warnings(&files, BuildTarget::Ruby);
    assert!(
        warnings.iter().any(|w| w.contains("mailer before_action") && w.contains("`__mailer_action_hello`")),
        "{warnings:#?}"
    );
}

// `skip_before_action` isn't lowered: the class keeps its calls reported.
#[test]
fn skip_before_action_keeps_the_callbacks_reported() {
    let mut files = app_files();
    let skipping = NOTICE_MAILER.replacen("  around_action :wrap\n", "  around_action :wrap\n  skip_before_action :only_hello\n", 1);
    let skipping: &'static str = Box::leak(skipping.into_boxed_str());
    files[2] = ("app/mailers/notice_mailer.rb", skipping);
    let warnings = lowered_warnings(&files, BuildTarget::Ruby);
    assert!(
        warnings.iter().any(|w| w.contains("mailer skip_before_action") && w.contains("`skip_before_action` is not lowered")),
        "{warnings:#?}"
    );
}

// A callback the mailer gets from an included module (oracle
// `test_app/concern_oracle.rb`): Rails finds it through the include.
const STAMPING: &str = "module Stamping\n  private\n\n  def stamp_concern\n    CallbackLog.add(\"stamp_concern\")\n    headers(\"X-Concern\" => \"1\")\n  end\nend\n";
const CONCERN_MAILER: &str = "class ConcernMailer < ApplicationMailer\n  include Stamping\n  after_action :stamp_concern\n\n  def hi\n    mail(to: \"a@example.com\", subject: \"Hi\", body: \"hi\")\n  end\nend\n";

#[test]
fn a_callback_from_an_included_module_runs_on_ruby() {
    let run = app()
        .write("app/mailers/concerns/stamping.rb", STAMPING)
        .write("app/mailers/concern_mailer.rb", CONCERN_MAILER)
        .run_ruby("CallbackLog.clear\nm = ConcernMailer.hi.deliver_now\ngot = [CallbackLog.entries, m[\"X-Concern\"].value, m[\"X-Stamp\"].value]\nraise got.inspect unless got == [[\"base_before\", \"stamp_concern\", \"stamp\"], \"1\", \"yes\"]\nputs \"concern passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("concern passed"), "{}", run.stdout);
}

// A public method is an action too, but called directly from a running
// action it is a plain method call: Rails runs no callbacks for it
// (oracle `test_app/direct_oracle.rb`).
const DIRECT_MAILER: &str = "class DirectMailer < ApplicationMailer\n  before_action :note_before\n\n  def hi\n    mail(to: \"a@example.com\", subject: greeting, body: \"hi\")\n  end\n\n  def greeting\n    CallbackLog.add(\"greeting\")\n    \"Hello\"\n  end\n\n  private\n\n  def note_before\n    CallbackLog.add(\"before\")\n  end\nend\n";

#[test]
fn an_action_called_from_a_running_action_runs_no_chain_on_ruby() {
    let run = app()
        .write("app/mailers/direct_mailer.rb", DIRECT_MAILER)
        .run_ruby("CallbackLog.clear\nm = DirectMailer.hi.deliver_now\ngot = [CallbackLog.entries, m.subject]\nraise got.inspect unless got == [[\"base_before\", \"before\", \"greeting\", \"stamp\"], \"Hello\"]\nputs \"direct passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("direct passed"), "{}", run.stdout);
}

// Review of #1383 (oracle `test_app/review_oracle.rb`): the yielded
// `with_options` form registers its calls, and two actions through one
// `with` each run their own chain on a fresh mail.
const REVIEW_MAILER: &str = "class ReviewMailer < ApplicationMailer\n  with_options only: :hello do |o|\n    o.before_action :note_hello\n  end\n  before_action :note_before\n\n  def hello\n    headers(\"X-Hello\" => \"1\")\n    mail(to: \"a@example.com\", subject: \"Hello\", body: \"h\")\n  end\n\n  def bye\n    mail(to: \"a@example.com\", subject: \"Bye\", body: \"b\")\n  end\n\n  private\n\n  def note_hello\n    CallbackLog.add(\"note_hello\")\n  end\n\n  def note_before\n    CallbackLog.add(\"before\")\n  end\nend\n";
const REVIEWS: &str = "class Reviews\n  def self.with(params)\n    ReviewMailer.with(params)\n  end\nend\n";

fn review_app() -> emit_and_run::Overlay {
    app().write("app/mailers/review_mailer.rb", REVIEW_MAILER).write("app/models/reviews.rb", REVIEWS)
}

#[test]
fn the_yielded_with_options_form_registers_its_calls_on_ruby() {
    let run = review_app().run_ruby(r##"
CallbackLog.clear
ReviewMailer.hello.deliver_now
got = CallbackLog.entries
raise "yielded with_options: #{got.inspect}" unless got == ["base_before", "note_hello", "before", "stamp"]
puts "yielded passed"
"##);
    run.assert_passes();
    assert!(run.stdout.contains("yielded passed"), "{}", run.stdout);
}
