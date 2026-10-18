
# Rails-dom-testing's `css_select`: the elements of the last response
# matching `selector`, as Nokogiri nodes (`node["id"]`). Appended to a
# spinel tree's harness only when it carries spinel-nokogiri and a test
# calls it (`project::apply_spinel_css_select`); the CRuby tree has its
# own in test_support_cruby.rb. rails-dom-testing parses with HTML5.
require "nokogiri"

class TestBase
  def css_select(selector)
    Nokogiri::HTML5.fragment(@__response.body.to_s).css(selector).to_a
  end
end
