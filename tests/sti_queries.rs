//! An STI subclass's queries (#1415), with a `type` column: real-blog's
//! articles with one, `Feature < Article`, `SpecialFeature < Feature`,
//! and the calls in app code (`StiProbe`), where `lower::sti_scope`
//! rewrites them. activerecord 8.1.4 (oracle `sti_queries_oracle.rb`,
//! which runs these classes and script): a subclass's rows include its
//! own subclasses' (`type IN ('Feature', 'SpecialFeature')`), and its
//! `find`, `sum`, `take`, `find_each`, `find_or_create_by`, `select`,
//! `none`, `pick` and the base's named scopes are scoped like `count`.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const FEATURE: &str = "class Feature < Article\nend\n";
const SPECIAL_FEATURE: &str = "class SpecialFeature < Feature\nend\n";
const STI_PROBE: &str = "class StiProbe\n  def self.seed\n    a = Article.create!(title: \"a\", body: \"a body long enough\")\n    f = Feature.create!(title: \"f\", body: \"f body long enough\")\n    s = SpecialFeature.create!(title: \"s\", body: \"s body long enough\")\n    [a.id, f.id, s.id]\n  end\n\n  def self.types\n    Article.order(:id).pluck(:type)\n  end\n\n  def self.counts\n    [Article.count, Feature.count, SpecialFeature.count]\n  end\n\n  def self.where_titles\n    Feature.where(title: [\"a\", \"f\", \"s\"]).order(:id).pluck(:title)\n  end\n\n  def self.scope_titles\n    Feature.titled_a_or_f.pluck(:title)\n  end\n\n  def self.id_sum\n    Feature.sum(:id)\n  end\n\n  def self.take_title\n    Feature.order(:id).take.title\n  end\n\n  def self.first_class\n    Feature.order(:id).first.class.name\n  end\n\n  def self.find_class(id)\n    Feature.find(id).class.name\n  rescue ActiveRecord::RecordNotFound\n    \"RecordNotFound\"\n  end\n\n  def self.find_by_title(title)\n    Feature.find_by(title: title)\n  end\n\n  def self.each_titles\n    titles = []\n    Feature.find_each { |record| titles << record.title }\n    titles\n  end\n\n  def self.found_or_built_type\n    record = Feature.find_or_create_by(title: \"n\")\n    record.type\n  end\n\n  def self.select_titles\n    Feature.select(:id, :title).order(:id).map(&:title)\n  end\n\n  def self.none_count\n    Feature.none.count\n  end\n\n  def self.exists(id)\n    Feature.exists?(id)\n  end\n\n  def self.pick_title\n    Feature.order(:id).pick(:title)\n  end\nend\n";
const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a, f, s = StiProbe.seed
check("each row's type", StiProbe.types, [nil, "Feature", "SpecialFeature"])
check("[Article, Feature, SpecialFeature].count", StiProbe.counts, [3, 2, 1])
check("where on the subclass", StiProbe.where_titles, ["f", "s"])
check("the base's scope on the subclass", StiProbe.scope_titles, ["f"])
check("sum", StiProbe.id_sum, f + s)
check("take", StiProbe.take_title, "f")
check("first", StiProbe.first_class, "Feature")
check("find, the subclass's row", StiProbe.find_class(f), "Feature")
check("find, a deeper subclass's row", StiProbe.find_class(s), "SpecialFeature")
check("find, the base's row", StiProbe.find_class(a), "RecordNotFound")
check("find_by, the base's row", StiProbe.find_by_title("a"), nil)
check("find_each", StiProbe.each_titles, ["f", "s"])
check("find_or_create_by builds the subclass's type", StiProbe.found_or_built_type, "Feature")
check("select", StiProbe.select_titles, ["f", "s"])
check("none", StiProbe.none_count, 0)
check("exists?, the base's row", StiProbe.exists(a), false)
check("pick", StiProbe.pick_title, "f")
puts "sti queries passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "create_table \"articles\", force: :cascade do |t|\n    t.string \"title\"\n",
            "create_table \"articles\", force: :cascade do |t|\n    t.string \"title\"\n    t.string \"type\"\n",
        )
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  scope :titled_a_or_f, -> { where(title: [\"a\", \"f\"]) }\n",
        )
        .write("app/models/feature.rb", FEATURE)
        .write("app/models/special_feature.rb", SPECIAL_FEATURE)
        .write("app/models/sti_probe.rb", STI_PROBE)
}

#[test]
fn an_sti_subclasss_queries_are_scoped_to_it_and_its_subclasses_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti queries passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_sti_subclasss_queries_are_scoped_to_it_and_its_subclasses_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti queries passed"), "{}", run.stdout);
}
