use roundhouse::build_target::BuildTarget;

use super::emit_and_run;

/// A program that defines `SVG` itself (svg-graph compiled from source)
/// does not also get the raising `SVG::Graph::TimeSeries` façade: two
/// definitions of one method are what spinel refuses.
#[test]
fn a_namespace_the_program_defines_leaves_the_gem_facade() {
    let app = emit_and_run::real_blog().write(
        "app/models/svg/graph/time_series.rb",
        "module SVG\n  module Graph\n    class TimeSeries\n      def get_x_values\n        [1]\n      end\n    end\n  end\nend\n",
    );
    let (emitted, _errors) = app.emit(BuildTarget::Spinel);
    let facades = std::fs::read_to_string(emitted.join("runtime/gem_facades.rb")).expect("gem_facades.rb");
    assert!(!facades.contains("module SVG"), "{facades}");
    assert!(facades.contains("module Markly"), "the other façades stay:\n{facades}");
    let sig = std::fs::read_to_string(emitted.join("runtime/gem_facades.rbs")).expect("gem_facades.rbs");
    assert!(!sig.contains("module SVG"), "{sig}");
}

/// Without it, the façade stays as it was.
#[test]
fn without_the_namespace_the_facade_stays() {
    let (emitted, _errors) = emit_and_run::real_blog().emit(BuildTarget::Spinel);
    let facades = std::fs::read_to_string(emitted.join("runtime/gem_facades.rb")).expect("gem_facades.rb");
    assert!(facades.contains("module SVG"), "{facades}");
}
