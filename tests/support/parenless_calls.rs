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
        .edit(
            "app/models/comment.rb",
            COMMENT_ANCHOR,
            &format!("{COMMENT_ANCHOR}\n  def self.tag\n    \"t\"\n  end\n\n  def self.tags\n    [tag, tag].join(\",\")\n  end\n"),
        )
}
