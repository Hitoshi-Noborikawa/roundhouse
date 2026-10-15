//! With rails-i18n in the bundle a count picks its form by the locale's
//! own rule (Polish `few`/`many`, Russian, Japanese `other` only, French
//! `one` below two), with I18n's Pluralization backend taking `zero`,
//! `0` and `1` first and `other` for a form an entry lacks. Expected
//! values are i18n 1.15.2's with rails-i18n 8.1.0's pluralization files
//! for the same translations on CRuby 3.4.

use super::emit_and_run;

const LOCALES_YML: &str = "en:
  probe:
    items:
      zero: \"no items\"
      one: \"%{count} item\"
      other: \"%{count} items\"
    files:
      one: \"%{count} file\"
      other: \"%{count} files\"
pl:
  probe:
    items:
      \"0\": \"brak\"
      one: \"%{count} rzecz\"
      few: \"%{count} rzeczy\"
      many: \"%{count} rzeczy (wiele)\"
      other: \"%{count} rzeczy (ułamek)\"
ru:
  probe:
    items:
      one: \"%{count} вещь\"
      few: \"%{count} вещи\"
      many: \"%{count} вещей\"
      other: \"%{count} вещи (дробь)\"
ja:
  probe:
    items:
      one: \"%{count} 個 (one)\"
      other: \"%{count} 個\"
fr:
  probe:
    items:
      one: \"%{count} objet\"
      other: \"%{count} objets\"
";

const PROBE: &str = "  validates :title, presence: true

  def self.items(locale, counts)
    I18n.with_locale(locale) do
      counts.map { |n| I18n.t(\"probe.items\", count: n) }
    end
  end

  def self.files(locale, count)
    I18n.with_locale(locale) do
      I18n.t(\"probe.files\", count: count)
    end
  end
";

fn plural_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("config/locales/probe.yml", LOCALES_YML)
        .edit("Gemfile.lock", "    i18n (1.15.2)\n      concurrent-ruby (~> 1.0)\n", "    i18n (1.15.2)\n      concurrent-ruby (~> 1.0)\n    rails-i18n (8.1.0)\n      i18n (>= 0.7, < 2)\n")
        .edit(
            "config/application.rb",
            "    # config.time_zone",
            "    config.i18n.available_locales = %i[en pl ru ja fr]\n    config.i18n.default_locale = :en\n    config.i18n.fallbacks = [ :en ]\n    # config.time_zone",
        )
        .edit("app/models/article.rb", "  validates :title, presence: true\n", PROBE)
}

const PLURAL_ASSERTIONS: &str = r##"expect = lambda { |what, got, want| raise "#{what}: #{got.inspect}" unless got == want }
expect.call("en", Article.items(:en, [0, 1, 2]), ["no items", "1 item", "2 items"])
expect.call("pl", Article.items(:pl, [0, 1, 2, 5, 12, 22, 1.5]), ["brak", "1 rzecz", "2 rzeczy", "5 rzeczy (wiele)", "12 rzeczy (wiele)", "22 rzeczy", "1.5 rzeczy (ułamek)"])
expect.call("ru", Article.items(:ru, [1, 3, 5, 11, 21, 1.5]), ["1 вещь", "3 вещи", "5 вещей", "11 вещей", "21 вещь", "1.5 вещи (дробь)"])
expect.call("ja", Article.items(:ja, [1, 2]), ["1 個", "2 個"])
expect.call("fr", Article.items(:fr, [0, 1, 1.5, 2]), ["0 objet", "1 objet", "1.5 objet", "2 objets"])
expect.call("fallback", [Article.files(:pl, 1), Article.files(:pl, 5)], ["1 file", "5 files"])
puts "plural rules passed"
"##;

#[test]
fn a_count_takes_the_form_its_locale_s_rule_picks() {
    plural_app().run_ruby(PLURAL_ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_count_takes_the_form_its_locale_s_rule_picks_on_spinel() {
    plural_app().run_spinel(PLURAL_ASSERTIONS).assert_passes();
}
