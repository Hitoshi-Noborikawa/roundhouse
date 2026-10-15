//! `truncate(text, separator:)` — rubys/roundhouse#812.
//!
//! `ActionView::ViewHelpers.truncate` (the shared runtime every
//! ruby-family target calls through) only accepted `length:` and
//! `omission:`. `src/lower/view_to_library/helpers.rs`'s `Truncate`
//! arm hands every option through unchanged, so a view's
//! `truncate(text, length: 20, separator: " ")` passed `check` with no
//! gap and the emitted program raised `ArgumentError: unknown keyword:
//! :separator` on every request.
//!
//! Rails' real algorithm (`ActiveSupport::core_ext::String#truncate`,
//! read from the `activesupport` gem installed on this machine):
//!
//! ```text
//! cutoff = length - omission.length
//! return omission if cutoff <= 0
//! cutoff = rindex(separator, cutoff) || cutoff if separator
//! "#{self[0, cutoff]}#{omission}"
//! ```
//!
//! which for `"Rails is a web application framework"` at `length: 20,
//! separator: " "` cuts at the space before "application" (index 14),
//! giving `"Rails is a web..."` — confirmed against Rails 8.1.4's own
//! `String#truncate` below, not just read from source.
use std::path::Path;
use std::process::Command;

fn runtime_file() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/ruby/action_view/view_helpers.rb")
}

#[test]
fn truncate_separator_matches_rails_8_1_4() {
    let script = r#"
require "active_support"
require "active_support/core_ext/string/filters"
load ARGV.fetch(0)

def assert(value, message)
  raise message unless value
end

cases = [
  ["Rails is a web application framework", 20, "...", " "],
  ["one two three four five", 12, "...", " "],
  ["noseparatorhere", 8, "...", " "],
  ["short", 30, "...", " "],
]
cases.each do |text, length, omission, separator|
  runtime_result = ActionView::ViewHelpers.truncate(text, length: length, omission: omission, separator: separator)
  rails_result = text.truncate(length, omission: omission, separator: separator)
  assert(
    runtime_result == rails_result,
    "truncate(#{text.inspect}, length: #{length}, separator: #{separator.inspect}) " \
    "runtime=#{runtime_result.inspect} rails=#{rails_result.inspect}"
  )
end

assert(
  ActionView::ViewHelpers.truncate("Rails is a web application framework", length: 20, separator: " ") == "Rails is a web...",
  "must match the #812 reduction's own Rails 8.1.4 output"
)
puts "truncate separator contract passed"
"#;
    let output = Command::new("ruby")
        .arg("-e")
        .arg(script)
        .arg(runtime_file())
        .output()
        .expect("ruby is on PATH");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("contract passed"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
