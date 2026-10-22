//! A zero-argument call written without parens on a plain class, a
//! subclass's class method and a model's class method (#1229).

use super::emit_and_run::{self, Overlay};

pub const METER: &str = r#"class Meter
  def self.label
    "base"
  end

  def self.a
    label.upcase
  end

  def self.b
    label
  end

  def self.c
    self.label.upcase
  end

  def name
    "n"
  end

  def e
    name.upcase
  end
end
"#;

/// A memoized class method, which the emit declares as a `static` field
/// (and leaves the method out): a read without parens stays a read.
pub const DIAL: &str = "class Dial\n  def self.mode\n    @mode ||= \"x\"\n  end\n\n  def self.loud\n    mode.upcase\n  end\nend\n";

/// An instance reader and a class method of the same name: Ruby keeps
/// them apart (`Knob.shout` reads the class method), and so must the
/// emit.
pub const KNOB: &str = "class Knob\n  attr_reader :label\n\n  def initialize\n    @label = \"inst\"\n  end\n\n  def self.label\n    \"cls\"\n  end\n\n  def self.shout\n    label.upcase\n  end\nend\n";

/// An instance method and a class predicate that sanitize to one name
/// (`is_ready`, `ready?`): each side keeps its own.
pub const LAMP: &str = "class Lamp\n  def is_ready\n    false\n  end\n\n  def self.ready?\n    true\n  end\n\n  def self.state\n    ready? ? \"on\" : \"off\"\n  end\nend\n";

pub const WIDGET: &str = "class Widget < Meter\n  def self.shout\n    label.upcase\n  end\nend\n";

const COMMENT_ANCHOR: &str = "  after_destroy_commit { article.broadcast_replace_to(\"articles\") rescue nil }\n";

/// What `[Meter.a, Meter.b, Meter.c, Meter.new.e, Widget.shout,
/// Comment.tags].join(",")` answers under CRuby.
#[allow(dead_code)]
pub const SUMMARY: &str = "BASE,base,BASE,N,BASE,t,t";

pub fn overlay() -> Overlay {
    emit_and_run::real_blog()
        .write("app/models/meter.rb", METER)
        .write("app/models/widget.rb", WIDGET)
        .write("app/models/dial.rb", DIAL)
        .write("app/models/knob.rb", KNOB)
        .write("app/models/lamp.rb", LAMP)
        .edit(
            "app/models/comment.rb",
            COMMENT_ANCHOR,
            &format!("{COMMENT_ANCHOR}\n  def self.tag\n    \"t\"\n  end\n\n  def self.tags\n    [tag, tag].join(\",\")\n  end\n"),
        )
}
