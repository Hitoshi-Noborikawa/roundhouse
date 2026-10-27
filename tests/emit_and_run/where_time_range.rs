use super::emit_and_run;

/// A `where` value that is a Range of Times (lobsters' Link scope reads
/// `created_at: 7.days.ago..`) is lowered to `ActiveRecord::Bounds` on the
/// ruby family: Spinel keeps a Range's bounds in integer slots. The SQL is
/// a Range's: `>=` for the start, `<` for an exclusive end.
fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "  validates :title, presence: true\n",
        "  validates :title, presence: true

  def self.recent_titles
    where(created_at: 7.days.ago..).order(:title).map(&:title)
  end

  def self.backdate(article)
    article.update_column(:created_at, 10.days.ago)
  end

  def self.cut
    3.days.ago
  end

  def self.older_titles(cut)
    where(created_at: ...cut).order(:title).map(&:title)
  end

  def self.recent_listed_titles
    where(created_at: [nil, 7.days.ago..]).order(:title).map(&:title)
  end

  def self.seeded(title)
    where(created_at: 7.days.ago.., title: title).first_or_initialize
  end
",
    )
}

pub const ASSERTIONS: &str = r#"Article.delete_all
Article.create!(title: "new", body: "A sufficiently long body.")
old = Article.create!(title: "old", body: "A sufficiently long body.")
Article.backdate(old)
raise "recent: #{Article.recent_titles.inspect}" unless Article.recent_titles == ["new"]
raise "older: #{Article.older_titles(Article.cut).inspect}" unless Article.older_titles(Article.cut) == ["old"]
raise "listed: #{Article.recent_listed_titles.inspect}" unless Article.recent_listed_titles == ["new"]
fresh = Article.seeded("fresh")
raise "seeded: #{fresh.title.inspect} #{fresh.created_at.inspect}" unless fresh.title == "fresh" && fresh.created_at.nil? && !fresh.persisted?
puts "where time range passed"
"#;

#[test]
fn a_time_range_condition_selects_like_rails() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let model = std::fs::read_to_string(emitted.join("app/models/article.rb")).expect("article.rb");
    assert!(model.contains("ActiveRecord::Bounds.new("), "{model}");
    let run = app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("where time range passed"));
}
