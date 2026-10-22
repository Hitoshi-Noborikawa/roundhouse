//! An app class named `Base`, and a class extending it, in an app whose
//! models and controllers extend the runtime's `ActiveRecord::Base` and
//! `ActionController::Base` (#1228).

use super::emit_and_run::{self, Overlay};

pub const BASE: &str = "class Base\n  LABEL = \"base\"\n\n  def self.label\n    LABEL\n  end\nend\n";
pub const WIDGET: &str = "class Widget < Base\nend\n";

/// What `[Widget.label, Widget::LABEL].join(",")` answers under CRuby.
#[allow(dead_code)]
pub const SUMMARY: &str = "base,base";

pub fn overlay() -> Overlay {
    emit_and_run::real_blog().write("app/models/base.rb", BASE).write("app/models/widget.rb", WIDGET)
}
