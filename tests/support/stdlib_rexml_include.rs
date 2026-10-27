//! A class that `include`s REXML and names its classes bare, as svg-graph
//! does, shared by the Ruby lane and the Spinel lane. Ruby finds `Comment`
//! in the included module before `Object`, so real-blog's own `Comment`
//! model is not the one built. The answers are CRuby 3.4's with rexml 3.4.

use super::emit_and_run;

pub const SVG_DOC: &str = r##"class SvgDoc
  include REXML

  def self.render
    doc = Document.new
    doc << XMLDecl.new
    root = doc.add_element("svg", { "width" => "10", "xmlns" => "http://www.w3.org/2000/svg" })
    root << Comment.new(" made here ")
    style = root.add_element("style", { "type" => "text/css" })
    style << CData.new("rect { fill: red }")
    rect = root.add_element("rect", { "x" => "1" })
    rect.text = "a < b"
    doc.to_s
  end

  def self.parsed_name
    REXML::Document.new("<a><b n='1'/></a>").root.elements[1].attributes["n"]
  end
end
"##;

pub const ASSERTIONS: &str = r##"want = "<?xml version='1.0'?><svg width='10' xmlns='http://www.w3.org/2000/svg'><!-- made here --><style type='text/css'><![CDATA[rect { fill: red }]]></style><rect x='1'>a &lt; b</rect></svg>"
got = SvgDoc.render
raise "render #{got.inspect}" unless got == want
raise "parsed" unless SvgDoc.parsed_name == "1"
puts "rexml passed"
"##;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/svg_doc.rb", SVG_DOC)
}
