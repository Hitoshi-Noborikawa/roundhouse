#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 1) do\nend\n",
        )
        .write(
            "app/models/upload_clock.rb",
            r#"class UploadClock
  MAX_AGE = 1.day
  SHIFT = 2.seconds

  def self.cutoff(now: Time.current)
    now - MAX_AGE
  end

  def self.shifted(now: Time.current)
    now + SHIFT
  end
end
"#,
        )
}

const ASSERTIONS: &str = r#"now = Time.at(100000)
raise "day" unless UploadClock.cutoff(now).to_i == 13600
raise "shift" unless UploadClock.shifted(now).to_i == 100002
puts "PASS"
"#;

#[test]
fn named_durations_are_converted_for_time_arithmetic() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let source = std::fs::read_to_string(emitted.join("app/models/upload_clock.rb")).unwrap();
    assert!(source.contains("MAX_AGE.to_i"), "{source}");
    assert!(source.contains("SHIFT.to_i"), "{source}");
}

#[test]
fn named_duration_values_match_time_arithmetic_on_ruby() {
    app().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
fn a_numeric_constant_in_a_nearer_namespace_keeps_fractional_seconds() {
    app()
        .write(
            "app/models/scoped_clock.rb",
            r#"class ClockOffset
  VALUE = 2.seconds
end

module ClockNamespace
  class ClockOffset
    VALUE = 0.5
  end

  class ScopedClock
    def self.shifted
      Time.at(10) + ClockOffset::VALUE
    end
  end
end
"#,
        )
        .run_ruby("raise \"fractional seconds changed\" unless ClockNamespace::ScopedClock.shifted.to_f == 10.5\n")
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn named_durations_run_with_emitted_rbs() {
    app().run_spinel_with_rbs(ASSERTIONS).assert_passes();
}
