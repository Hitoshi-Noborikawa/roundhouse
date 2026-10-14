# `I18n.locale` / `I18n.default_locale` / `I18n.available_locales`, the
# locale a request runs in, and the answers for a key the locale lacks.
#
# Translation is folded at compile time (`src/lower/i18n_translate.rs`),
# once per locale the app makes available; a call that can run in more
# than one picks its branch by `I18n.locale` when it runs. The locale is
# per thread, as the gem holds it (`I18n.config` is thread-local), and
# is the default until something sets it. The app's default and
# available locales are spelled into the generated block below
# (`project::apply_i18n_locales`); an app that sets none keeps Rails'
# `:en`.
#
# `I18n.with_locale(l) { … }` returns its block's value, which no
# signature here can type, so `lower::i18n_translate` spells each call
# site out over `enter_locale` / `leave_locale`.
module I18n
  # `I18n.locale = :xx` for a locale the app does not make available
  # (the gem's `enforce_available_locales`, true by default).
  class InvalidLocale < ArgumentError
  end

  def self.default_locale
    app_default_locale
  end

  def self.available_locales
    app_available_locales
  end

  # The locale set for this thread, by name ("" when none is): a Symbol
  # read back out of `Thread.current` is a value the strict targets
  # cannot narrow, and a String they can. (Not `locale_set`: Spinel
  # spells `locale=` with that name.)
  def self.thread_locale_name
    Thread.current[:roundhouse_i18n_locale].to_s
  end

  def self.locale
    set = thread_locale_name
    set.empty? ? app_default_locale : set.to_sym
  end

  # `I18n.locale = x`: an available locale, a Symbol or its String.
  def self.locale=(value)
    Thread.current[:roundhouse_i18n_locale] = checked_locale(value).to_s
  end

  # `value` as the locale it names, or the gem's InvalidLocale, whose
  # message shows the value as `inspect` does (nil is not a locale).
  def self.checked_locale(value)
    name = value.to_s.to_sym
    return name if !value.nil? && app_available_locales.include?(name)
    shown = value.nil? ? "nil" : (value.is_a?(Symbol) ? ":" + value.to_s : "\"" + value.to_s + "\"")
    raise InvalidLocale, shown + " is not a valid locale"
  end

  # A call's `locale:` read when it runs, by name: nil is the current
  # locale, anything else must be available.
  def self.locale_name(value)
    value.nil? ? locale.to_s : checked_locale(value).to_s
  end

  # `with_locale(l)`'s first half: nil changes nothing and answers nil,
  # anything else is set and the locale it replaces answered, for
  # `leave_locale` to put back however the block ends.
  def self.enter_locale(value)
    return nil if value.nil?
    previous = locale.to_s
    Thread.current[:roundhouse_i18n_locale] = checked_locale(value).to_s
    previous
  end

  def self.leave_locale(previous)
    Thread.current[:roundhouse_i18n_locale] = previous unless previous.nil?
    nil
  end

  # Whether the running environment is one of `envs` (comma-separated),
  # where `config.i18n.fallbacks` differs by environment.
  def self.running_in?(envs)
    envs.split(",").include?(Rails.env.to_s)
  end

  class MissingTranslationData < ArgumentError
  end

  # Whether this environment raises on a missing translation: `raises`
  # is `config/application.rb`'s setting, and the comma-separated
  # `flipped` environments set the other value in their own file.
  def self.missing_raises(raises, flipped)
    env = Rails.env.to_s
    flipped.split(",").include?(env) ? !raises : raises
  end

  # A run-time key as `I18n.normalize_keys` reads it: dot-separated,
  # empty segments dropped (`"a..b."` is `a.b`).
  def self.normalize_key(key)
    out = []
    key.split(".").each { |part| out << part unless part.empty? }
    out.join(".")
  end

  # A run-time key that names a subtree, which `I18n.t` answers as a
  # Hash; no folded translation carries one.
  def self.subtree_translation(key)
    raise NotImplementedError, "I18n.t(\"#{key}\") answers a Hash of translations, which is not supported"
  end

  # `I18n.t`'s answer for `key` (`en.shop.title`): I18n's default
  # handler returns the message, `raise_on_missing_translations` raises it.
  def self.missing_translation(key, raises)
    message = "Translation missing: " + key
    raise MissingTranslationData, message if raises
    message
  end

  # The same answer when the call's `default:` was a list of keys none of
  # which is there either: I18n lists every key it considered, `rest`
  # being the defaults' lines (`"\n- en.posts.title"`).
  def self.missing_translation_among(key, rest, raises)
    message = "Translation missing. Options considered were:\n- " + key + rest
    raise MissingTranslationData, message if raises
    message
  end

  # A template's `t` for `key`: Action View's debug span, titled with the
  # key and the call's options (`details`, each value escaped once), and
  # reading as the key's last segment titleized.
  def self.missing_translation_tag(key, details, raises)
    missing_translation(key, raises)
    last = key.split(".").last.to_s
    title = ActionView::ViewHelpers.html_escape("translation missing: " + key + details)
    text = ActionView::ViewHelpers.html_escape(ActiveSupport.titleize(last))
    "<span class=\"translation_missing\" title=\"" + title + "\">" + text + "</span>"
  end

  # >>> generated: i18n-locales
  def self.app_default_locale
    :en
  end

  def self.app_available_locales
    [:en]
  end
  # <<< generated: i18n-locales
end
