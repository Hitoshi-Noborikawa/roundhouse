# `roundhouse.yml` — app configuration

Most apps need no configuration. Roundhouse reads the app as it is, and
an app without a `roundhouse.yml` gets the default behavior.

Add the file at the app root, next to `Gemfile`, when you need to tell
Roundhouse something it cannot learn from the source. The file has two
keys:

```yaml
test_paths:
  - test/unit
extensions:
  - config/roundhouse
```

- [`test_paths`](#test_paths) adds test folders to the default list.
- [`extensions`](#extensions) lists directories that describe the app's
  gems and class-body DSLs.

Roundhouse reads the file during ingest, so every tool sees it:
[`roundhouse check`](check.md), the [editor](editor.md), the
[MCP server](mcp.md), the [in-browser IDE](ide.md) (from the folder you
open), and [`roundhouse --target`](transpile.md).

## Errors

A mistake in `roundhouse.yml` or in an extension file stops ingestion.
Roundhouse does not ignore the entry or guess what it meant. Unknown keys
are errors too, so a typo fails loudly:

```
$ roundhouse check .
roundhouse-check: ingest failed: parse error in ./roundhouse.yml: unknown field `extension`, expected `test_paths` or `extensions`
```

`check` exits 2 on these errors. `--continue` does not get past them,
even though the hint line suggests it.

## `test_paths`

Roundhouse reads every `.rb` file recursively from these folders by
default:

- `test/models`
- `test/controllers`
- `test/helpers`
- `test/channels`
- `test/lib`

`test_paths` adds folders:

```yaml
test_paths:
  - test/unit
  - quality/specs
```

`test_paths` adds folders. It does not replace the default folders. Each
value must name a literal directory relative to the app root. Roundhouse
does not expand globs, environment variables, or `~`.
Roundhouse rejects a configured path with a symbolic link. It does not
follow symbolic links below selected folders.

Roundhouse reads `test/test_helper.rb`, `test/test_helpers/`, and
`test/fixtures/` outside this folder list.

Transpiled projects include tests from the default folders and these
folders.

## `extensions`

Roundhouse models Rails and a set of common gems. Project extensions
describe other gems and class-body DSLs that Roundhouse does not know
about. The descriptions live in the app's own tree, so the compiler does
not have to carry them.

### When you need one

**A gem's value object fails at every use.** A gem adds a class-body
macro that turns columns into value objects:

```ruby
class Invoice < ApplicationRecord
  price_column :amount, currency: :usd

  def label = amount.currency
end
```

Roundhouse types `amount` from the schema, so each use of the value
object is an error:

```
app/models/invoice.rb:4:15: error[send_dispatch_failed]: no known method `currency` on Float?
```

An [`attribute_macros`](#macrosyml) entry makes the attribute untyped
and reports one error at the macro instead:

```
app/models/invoice.rb:2:3: error[unsupported]: price_column not supported (all targets): attribute macro on `Invoice` from a project extension: its attributes read untyped, and no target lowers the call
```

The error remains because no target translates the macro yet, but it
now names the cause once, rather than one error for each use.

**The `ruby` tree needs a gem it does not load.** The `ruby` and
`jruby` targets carry many of the app's gems, but not every gem is
required at boot or declared in the emitted Gemfile.
[`gems.yml`](#gemsyml) tells those targets how to load or declare a gem,
or records that the gem cannot load without Rails. It does not teach
`check` the gem's types. A constant from a gem Roundhouse does not
model is still an error:

```
app/services/report.rb:3:5: error[unsupported]: constant not supported (all targets): WidgetEngine::Thing — unmodeled gem `widget_engine`; emitted constant availability is unverified
```

### Directories

List extension directories in `roundhouse.yml`:

```yaml
extensions:
  - config/roundhouse
```

Each entry names a literal directory relative to the app root, as with
`test_paths`. It must not contain `..`, an absolute path, or a symbolic
link in the directory path. Directories load in the listed order.
The same directory must not appear twice.

A directory can hold `gems.yml` and `macros.yml`. Both files are
optional. Each file can be a symbolic link, including a link to a file
outside the app root, so several apps can share one description of a
gem. Roundhouse reports an error if it cannot read a linked file.

### `gems.yml`

```yaml
lock_gated_requires:
  - {require: "widgets/all", gem: "widgets"}
rails_dependent:
  - {gem: "widget-engine", why: "requiring it loads Rails framework files"}
runtime_gems:
  - {constant: "Widget", gem: "widgets"}
  - {any_text: ["Widget::Cache."], gem: "widget-cache"}
```

- `lock_gated_requires`: load this gem when the app starts. When the
  app's `Gemfile.lock` resolves the gem from a gem server, the `ruby`
  tree pins it at the locked version and requires the path at boot.
  A gem locked from git or a path is skipped.
- `rails_dependent`: requiring the gem loads Rails, so no tree loads it.
  The `ruby` tree does not carry it from the lockfile, and its constants
  stay errors. `roundhouse --target` adds the `why` text to those errors;
  `check` does not.
- `runtime_gems`: the `ruby` and `jruby` trees declare the gem in their
  Gemfile when a file under `app/` names the constant or contains one of
  the texts. This only declares the gem; it does not require it.

These facts change what the `ruby` and `jruby` trees load and declare.
They do not add type information. Errors for the gem's constants in
`check` output are the same with or without them.

### `macros.yml`

```yaml
attribute_macros:
  - {name: price_column, positional: true}     # price_column :a, :b, opt: 1
  - {name: bag_columns, hash_key: attributes}  # bag_columns attributes: [:a, :b]
```

`attribute_macros`: the attributes a macro names read and write as
untyped. With `positional`, the names are the macro's leading symbol
arguments. With `hash_key`, they are the array under that keyword. No
target lowers the macro itself, so each call is reported as an error at
its declaration. This replaces a dispatch error at every place the value
object is used.

Configured attribute macros also apply in a concern's `included do`
block. Roundhouse retains direct statements from `ActiveSupport::Concern`
blocks and inserts them into each including model before analysis.
Each model gets the macro's attribute types and its declaration error.
A concern that no model includes does not change attribute types or
produce macro declaration errors. This does not add general class-body
execution or support for `class_attribute` on models.

### Rules

Extension rows are added after Roundhouse's own rows and never replace
them. Unknown keys, malformed rows, a missing directory, and a
`rails_dependent` gem that another fact loads or that Roundhouse models
all stop ingestion with an error that names the file. The `types` and
`overlay` entries are reserved and rejected until they are implemented.

The CLI and browser use the same built-in gem tables to validate
extensions.

Extension facts belong to the app that lists them. An app that lists no
extensions behaves as before.
