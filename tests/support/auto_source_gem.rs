//! An installed plain-Ruby gem the compiler neither models nor ports,
//! shared by the Ruby lane and the Spinel lane: the Spinel tree compiles
//! its source with the app, along with the gem it declares and names,
//! while the ruby family keeps loading the real gem. The Spinel tree
//! carries only the part the app reaches.

use super::emit_and_run;

const GEMS: &str = "vendor/bundle/ruby/3.4.0/gems";

pub fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "Gemfile.lock",
            "    debug (1.11.1)\n",
            "    exclaim (0.1)\n    greetings (1.0)\n      exclaim\n    debug (1.11.1)\n",
        )
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  greetings\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write(&format!("{GEMS}/greetings-1.0/lib/greetings.rb"), "require \"exclaim\"\nrequire \"greetings/formatter\"\nrequire \"greetings/bindings\"\nrequire \"greetings/card\"\n")
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/formatter.rb"),
            "module Greetings\n  class Formatter\n    def self.shout(text)\n      Exclaim.mark(text.upcase)\n    end\n\n    def self.bound(values)\n      Bindings.new(values)\n    end\n  end\nend\n",
        )
        // Per-object methods, as parslet's `Context` defines them: spinel refuses the shape, and
        // only `Formatter.bound`, which the app never calls, reaches it.
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/bindings.rb"),
            "module Greetings\n  class Bindings\n    def initialize(values)\n      values.each do |key, value|\n        singleton_class.send(:define_method, key) { value }\n      end\n    end\n  end\nend\n",
        )
        // Option writers reached only by a name built at run time, as svg-graph's
        // `init_with` reaches `TimeSeries#min_x_value=`.
        .write(
            &format!("{GEMS}/greetings-1.0/lib/greetings/card.rb"),
            "module Greetings\n  class Panel\n    attr_accessor :width\n\n    def initialize(options)\n      options.each { |key, value| send(key.to_s + \"=\", value) }\n    end\n  end\n\n  class Card < Panel\n    protected\n\n    def width=(value)\n      @width = value.to_i * 2\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEMS}/exclaim-0.1/lib/exclaim.rb"),
            "module Exclaim\n  def self.mark(text)\n    text + \"!\"\n  end\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  def shouted_title\n    Greetings::Formatter.shout(title)\n  end\n\n  def card_width\n    Greetings::Card.new({ width: \"7\" }).width\n  end\n\n",
        )
}

pub const ASSERTIONS: &str = r#"got = Article.new(title: "hello", body: "a body that is long enough").shouted_title
raise "got #{got.inspect}" unless got == "HELLO!"
width = Article.new(title: "hello", body: "a body that is long enough").card_width
raise "width #{width.inspect}" unless width == 14
puts "auto source gem OK"
"#;
