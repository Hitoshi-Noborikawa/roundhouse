# frozen_string_literal: true
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

  class MissingInterpolationArgument < ArgumentError
  end

  class InvalidPluralizationData < ArgumentError
  end

  # `I18n.t(key, **options)` as the gem runs it, for a call whose key,
  # scope, default or options are known only when it runs: the key under
  # its scope (a view's or controller's lazy `.key` under `lazy_scope`),
  # each locale of the fallback chain, then each `default:`, and the
  # missing answer when none is found.
  def self.translate_runtime(key, options, lazy_scope, in_view, raises)
    loc = options.key?(:locale) ? locale_name(options[:locale]) : locale.to_s
    scope = scope_text(options[:scope])
    name = key.to_s
    name = lazy_scope + name if name.start_with?(".") && !lazy_scope.empty?
    full = normalize_key(scope.empty? ? name : scope + "." + name)
    count = options[:count]
    found = runtime_lookup(loc, full, count)
    considered = []
    defaults = options[:default]
    if found.nil? && !defaults.nil?
      list = defaults.is_a?(Array) ? defaults : [defaults]
      list.each do |default|
        next unless found.nil?
        if default.is_a?(Symbol)
          k = normalize_key(scope.empty? ? default.to_s : scope + "." + default.to_s)
          considered << loc + "." + k
          found = runtime_lookup(loc, k, count)
        elsif !default.nil?
          found = default.to_s
        end
      end
    end
    html = in_view && html_safe_key?(full)
    if found.nil?
      shown = loc + "." + full
      return missing_translation_tag(shown, "", raises) if in_view
      if defaults.is_a?(Array) && !considered.empty?
        return missing_translation_among(shown, considered.map { |c| "\n- " + c }.join, raises)
      end
      return missing_translation(shown, raises)
    end
    out = interpolate_runtime(found, options, html)
    in_view && !html ? ActionView::ViewHelpers.html_escape(out) : out
  end

  def self.scope_text(scope)
    return "" if scope.nil?
    return scope.map { |part| part.to_s }.join(".") if scope.is_a?(Array)
    scope.to_s
  end

  def self.html_safe_key?(key)
    last = key.split(".").last.to_s
    last == "html" || last.end_with?("_html")
  end

  # The translation `full` names in the first locale of `loc`'s chain that
  # has it: a leaf, or the plural form `count` picks from its children.
  def self.runtime_lookup(loc, full, count)
    leaves = runtime_leaves
    runtime_chain(loc).each do |l|
      text = leaves[l + "." + full]
      return text unless text.nil?
      next unless runtime_nodes.key?(l + "." + full)
      return plural_form(leaves, l + "." + full, count) unless count.nil?
      subtree_translation(full)
    end
    nil
  end

  # Not a locale's own plural rule: I18n's default picks zero (when given), one or other.
  def self.plural_form(leaves, node, count)
    zero = leaves[node + ".zero"]
    return zero if count == 0 && !zero.nil?
    form = count == 1 ? "one" : "other"
    text = leaves[node + "." + form]
    raise InvalidPluralizationData, "translation data #{node} can not be used with :count => #{count}. key '#{form}' is missing." if text.nil?
    text
  end

  def self.runtime_chain(loc)
    chains = runtime_chains
    env = Rails.env.to_s
    chains[loc + "\u001C" + env] || chains[loc + "\u001C"] || [loc]
  end

  # `%{name}` filled from the options that are not the gem's reserved ones, as I18n.interpolate does; none given leaves the text as it is.
  def self.interpolate_runtime(text, options, html)
    reserved = [:cascade, :deep_interpolation, :skip_interpolation, :default, :exception_handler, :fallback, :fallback_in_progress, :fallback_original_locale, :format, :object, :raise, :resolve, :scope, :separator, :throw, :locale]
    given = options.reject { |k, _| reserved.include?(k) }
    return text if given.empty?
    out = ""
    i = 0
    while i < text.length
      c = text[i, 1].to_s
      if c == "%" && text[i + 1, 1].to_s == "%"
        out = out + "%"
        i += 2
      elsif c == "%" && text[i + 1, 1].to_s == "{"
        close = text.index("}", i)
        if close.nil?
          out = out + c
          i += 1
        else
          name = text[i + 2, close - i - 2].to_s
          sym = name.to_sym
          raise MissingInterpolationArgument, "missing interpolation argument :#{name} in #{text.inspect} (#{given.inspect} given)" unless given.key?(sym)
          value = given[sym].to_s
          out = out + (html ? ActionView::ViewHelpers.html_escape(value) : value)
          i = close + 1
        end
      else
        out = out + c
        i += 1
      end
    end
    out
  end

  # Not folded per locale at the call: an app with a hundred locales made every `t` a hundred-arm case.
  def self.translate_site(id, loc, key, count, names, values, details, raises, in_view, escape_plain)
    entry = site_entry(id, loc, key)
    kind = entry[0]
    html = entry[1] == "1"
    if kind == "m"
      shown = entry[2].empty? ? loc + "." + key : entry[2]
      return missing_translation_tag(shown, details, raises) if in_view
      return entry[3].empty? ? missing_translation(shown, raises) : missing_translation_among(shown, entry[3], raises)
    end
    return subtree_translation(key) if kind == "s"
    template = entry[2]
    if kind == "p"
      template = (count == 0 && entry[2] == "1") ? entry[3] : (count == 1 ? entry[4] : entry[5])
    end
    out = render_template(template, count, names, values, html)
    escape_plain && !html ? ActionView::ViewHelpers.html_escape(out) : out
  end

  # A locale without records of its own answers as the default locale: the table leaves out each one that would repeat it.
  def self.site_entry(id, loc, key)
    found = site_entry_in(id, loc, key)
    found = site_entry_in(id, app_default_locale.to_s, key) if found.nil?
    raise ArgumentError, "no translation table entry for " + id + " in " + loc if found.nil?
    translation_entries[found]
  end

  def self.site_entry_in(id, loc, key)
    table = translation_table
    env = Rails.env.to_s
    sep = "\u001C"
    # Not the any-environment key before this environment's "*": an environment with entries of its own has them all.
    found = table[id + sep + loc + sep + env + sep + key]
    found = table[id + sep + loc + sep + env + sep + "*"] if found.nil? && !key.empty?
    found = table[id + sep + loc + sep + sep + key] if found.nil?
    found = table[id + sep + loc + sep + sep + "*"] if found.nil? && !key.empty?
    found
  end

  def self.render_template(template, count, names, values, html)
    out = ""
    template.split("\u001D").each do |part|
      next if part.empty?
      body = part[1, part.length - 1].to_s
      if part[0, 1] == "n"
        at = names.index(body)
        value = at.nil? ? (body == "count" ? count.to_s : "%{" + body + "}") : values[at].to_s
        out = out + (html ? ActionView::ViewHelpers.html_escape(value) : value)
      else
        out = out + body
      end
    end
    out
  end

  def self.translation_table
    load_translation_table if @translation_table.nil?
    @translation_table || {}
  end

  def self.runtime_leaves
    load_translation_table if @runtime_leaves.nil?
    @runtime_leaves || {}
  end

  def self.runtime_nodes
    load_translation_table if @runtime_nodes.nil?
    @runtime_nodes || {}
  end

  def self.runtime_chains
    load_translation_table if @runtime_chains.nil?
    @runtime_chains || {}
  end

  def self.translation_entries
    load_translation_table if @translation_entries.nil?
    @translation_entries || []
  end

  # A line ends in a "." field: `split` drops trailing empty fields, and a template or a key can be empty.
  def self.load_translation_table
    table = {}
    entries = []
    keys = {}
    leaves = {}
    nodes = {}
    chains = {}
    sep = "\u001C"
    File.read(File.join(Rails.root.to_s, "config", "i18n_translations")).split("\u001E").each do |line|
      fields = line.split("\u001F")
      kind = fields[0]
      if kind == "E"
        entries << fields[1, fields.length - 2]
      elsif kind == "K"
        keys[fields[1]] = fields[2, fields.length - 3]
      elsif kind == "L"
        leaves[fields[1]] = fields[2].to_s
        parts = fields[1].split(".")
        j = parts.length - 1
        while j > 1
          nodes[parts[0, j].join(".")] = true
          j -= 1
        end
      elsif kind == "F"
        chains[fields[1] + sep + fields[2]] = fields[3].to_s.split(",")
      elsif kind == "B"
        names = keys[fields[1]] || []
        prefix = fields[1] + sep + fields[2] + sep + fields[3] + sep
        i = 0
        while i < names.length
          at = fields[4 + i]
          table[prefix + names[i]] = at.to_i unless at == "-"
          i += 1
        end
      end
    end
    @runtime_leaves = leaves
    @runtime_nodes = nodes
    @runtime_chains = chains
    @translation_entries = entries
    @translation_table = table
    nil
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
