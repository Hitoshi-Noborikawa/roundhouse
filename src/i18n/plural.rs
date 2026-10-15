//! A locale's own plural rule, as the runtime evaluates it: prefix text
//! over the count's operands. A key is `:form` or `? <bool> <key> <key>`;
//! a bool is `& | !` over bools, `num` (the count is Numeric), `int x`,
//! `== != >= < x y`, `in x lo hi`, `inz x lo hi` (an integer in the
//! range, as `(lo..hi).to_a.include?` answers), or `of x k a1 … ak`; a
//! number is `% x y`, `floor x`, an operand or a literal. The operands
//! are rails-i18n's `r` (the count) and `d` (a fraction's digits), and
//! CLDR's `n i v f t`.

/// rails-i18n 8.1.0's `rails/pluralization/<locale>.rb`: the rule each
/// locale's file installs.
const RAILS_I18N: &[(&str, &str)] = &[
    ("ar", "Arabic"), ("az", "Other"), ("be", "EastSlavic"), ("bg", "OneOther"), ("bn", "OneOther"), ("bs", "OneFewOther"), ("ca", "OneOther"), ("cs", "WestSlavic"), ("da", "OneOther"), ("de-AT", "OneOther"), ("de-CH", "OneOther"), ("de-DE", "OneOther"), ("de", "OneOther"), ("el-CY", "OneOther"), ("el", "OneOther"), ("en-AU", "OneOther"), ("en-CA", "OneOther"), ("en-CY", "OneOther"), ("en-GB", "OneOther"), ("en-IE", "OneOther"), ("en-IN", "OneOther"), ("en-NZ", "OneOther"), ("en-TT", "OneOther"), ("en-US", "OneOther"), ("en-ZA", "OneOther"), ("en", "OneOther"), ("eo", "OneOther"), ("es-419", "OneOther"), ("es-AR", "OneOther"), ("es-CL", "OneOther"), ("es-CO", "OneOther"), ("es-CR", "OneOther"), ("es-EC", "OneOther"), ("es-ES", "OneOther"), ("es-MX", "OneOther"), ("es-NI", "OneOther"), ("es-PA", "OneOther"), ("es-PE", "OneOther"), ("es-US", "OneOther"), ("es-VE", "OneOther"), ("es", "OneOther"), ("et", "OneOther"), ("eu", "OneOther"), ("fa", "Other"), ("fi", "OneOther"), ("fr-CA", "OneUptoTwoOther"), ("fr-CH", "OneUptoTwoOther"), ("fr-FR", "OneUptoTwoOther"), ("fr", "OneUptoTwoOther"), ("fy", "OneOther"), ("gd", "ScottishGaelic"), ("gl", "OneOther"), ("he", "OneOther"), ("hi-IN", "OneWithZeroOther"), ("hi", "OneWithZeroOther"), ("hr", "OneFewOther"), ("hsb", "UpperSorbian"), ("hu", "OneOther"), ("id", "Other"), ("is", "OneOther"), ("it-CH", "OneOther"), ("it", "OneOther"), ("ja", "Other"), ("ka", "OneOther"), ("kk", "OneOther"), ("km", "Other"), ("kn", "Other"), ("ko", "Other"), ("lo", "Other"), ("lt", "Lithuanian"), ("lv", "Latvian"), ("mg", "OneWithZeroOther"), ("mk", "Macedonian"), ("ml", "OneWithZeroOther"), ("mn", "OneOther"), ("mr-IN", "OneWithZeroOther"), ("ms", "Other"), ("nb", "OneOther"), ("ne", "OneOther"), ("nl", "OneOther"), ("nn", "OneOther"), ("oc", "OneOther"), ("or", "OneWithZeroOther"), ("pa", "OneWithZeroOther"), ("pap-AW", "Other"), ("pap-CW", "Other"), ("pl", "Polish"), ("pt-BR", "OneOther"), ("pt", "OneOther"), ("ro", "Romanian"), ("ru", "EastSlavic"), ("sc", "OneOther"), ("sk", "WestSlavic"), ("sl", "Slovenian"), ("sr", "OneFewOther"), ("st", "OneOther"), ("sv-SE", "OneOther"), ("sv", "OneOther"), ("sw", "OneOther"), ("ta", "OneOther"), ("th", "Other"), ("tl", "OneWithZeroOther"), ("tr", "OneOther"), ("uk", "EastSlavic"), ("ur", "OneOther"), ("vi", "Other"), ("wo", "Other"), ("zh-CN", "Other"), ("zh-HK", "Other"), ("zh-TW", "Other"), ("zh-YUE", "Other"),
];

/// rails-i18n's `RailsI18n::Pluralization` rules. Each answers `other`
/// for a count that is not Numeric, which the guard every one starts
/// with says once.
fn rails_i18n_rule(name: &str) -> Option<&'static str> {
    Some(match name {
        "Other" => ":other",
        "OneOther" => "? == r 1 :one :other",
        "OneUptoTwoOther" => "? & >= r 0 < r 2 :one :other",
        "OneWithZeroOther" => "? of r 2 0 1 :one :other",
        "WestSlavic" => "? == r 1 :one ? of r 3 2 3 4 :few :other",
        "Romanian" => "? == r 1 :one ? | == r 0 inz % r 100 1 19 :few :other",
        "EastSlavic" => "? & == % r 10 1 != % r 100 11 :one ? & inz % r 10 2 4 ! inz % r 100 12 14 :few ? | | == % r 10 0 inz % r 10 5 9 inz % r 100 11 14 :many :other",
        "OneFewOther" => "? & == % d 10 1 != % d 100 11 :one ? & inz % d 10 2 4 ! inz % d 100 12 14 :few :other",
        "Arabic" => "? == r 0 :zero ? == r 1 :one ? == r 2 :two ? inz % r 100 3 10 :few ? inz % r 100 11 99 :many :other",
        "ScottishGaelic" => "? | == floor r 1 == floor r 11 :one ? | == floor r 2 == floor r 12 :two ? in floor r 3 19 :few :other",
        "UpperSorbian" | "Slovenian" => "? == % r 100 1 :one ? == % r 100 2 :two ? of % r 100 2 3 4 :few :other",
        "Lithuanian" => "? & == % r 10 1 ! inz % r 100 11 19 :one ? & inz % r 10 2 9 ! inz % r 100 11 19 :few :other",
        "Latvian" => "? & == % r 10 1 != % r 100 11 :one :other",
        "Macedonian" => "? & == % r 10 1 != r 11 :one :other",
        "Polish" => "? == r 1 :one ? & of % r 10 3 2 3 4 ! of % r 100 3 12 13 14 :few ? | of % r 10 7 0 1 5 6 7 8 9 of % r 100 3 12 13 14 :many :other",
        _ => return None,
    })
}

/// The rule rails-i18n installs for `locale`, if it has a file for it.
pub fn rails_i18n(locale: &str) -> Option<String> {
    let (_, name) = RAILS_I18N.iter().find(|(l, _)| *l == locale)?;
    Some(format!("? ! num :other {}", rails_i18n_rule(name)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rails_i18n_locale_names_a_rule() {
        for (locale, name) in RAILS_I18N {
            assert!(rails_i18n_rule(name).is_some(), "{locale}: {name}");
        }
        assert_eq!(rails_i18n("ja").as_deref(), Some("? ! num :other :other"));
        assert_eq!(rails_i18n("xx"), None);
    }
}
