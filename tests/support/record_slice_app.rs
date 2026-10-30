//! `record.slice(...)` on every target: the typed sites the
//! `record_slice` lowering grounds into a Hash of reader calls. No
//! database: the records are unsaved, so each toolchain only has to
//! compile and run the model. `payload` (the implicit-self site) is
//! compiled, not called: go renders a bare zero-arg call on a local as
//! a field read, so a call would test that heuristic, not `slice`.

use super::emit_and_run;

pub const PROBE: &str = "  validates :title, presence: true

  def payload = { action: \"start\", user: slice(:id, :title, :created_at) }

  def self.sliced
    a = Article.new(id: 7, title: \"Sliced\", body: \"A sufficiently long body.\")
    s = a.slice(:id, :title)
    t = Article.new(id: 8, title: \"Other\", body: \"Another long body.\").slice([:title])
    [s.size.to_s, s[\"title\"].to_s, s[\"id\"].to_s, t.size.to_s, t[\"title\"].to_s, a.slice(:id, :title, :created_at).size.to_s]
  end
";

/// What CRuby answers for `Article.sliced`, as Rails would.
pub const EXPECTED: [&str; 6] = ["2", "Sliced", "7", "1", "Other", "3"];

pub fn probe_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit("app/models/article.rb", "  validates :title, presence: true\n", PROBE)
}
