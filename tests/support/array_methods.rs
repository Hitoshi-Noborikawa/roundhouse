//! Array methods the TypeScript emit translates, on a typed `Integer` and
//! `String` Array, with the empty-Array and not-found answers Ruby gives.

use super::emit_and_run::{self, Overlay};

pub const PROBE: &str = r#"class ArrayProbe
  def self.run(xs, words)
    none = xs.select { |x| x > 9 }
    seen = []
    xs.each_with_index { |x, i| seen.push(x * i) }
    [
      xs.sum, xs.sum { |x| x * 2 }, none.sum,
      xs.select { |x| x > 1 }, xs.filter { |x| x != 2 }, xs.reject { |x| x > 1 },
      xs.collect { |x| x + 1 }, xs.detect { |x| x > 1 }, xs.detect { |x| x > 9 },
      xs.flat_map { |x| [x, x] }, [nil, 1, nil, 2].compact, [1, 1, 2, 3, 3].uniq,
      xs.max, xs.min, none.max, none.min,
      xs.first(2), xs.take(5), xs.drop(1), xs.drop(9), xs.last(2), xs.last(0), xs.dup,
      xs.count { |x| x > 1 }, [1, 2, 1].count(1),
      xs.inject(10) { |a, x| a + x }, xs.reduce { |a, x| a * x }, none.inject { |a, x| a + x },
      xs.index(2), xs.index(9), xs.index { |x| x > 2 }, words.find_index { |w| w == "b" },
      seen
    ]
  end
end
"#;

/// `JSON.generate(ArrayProbe.run([1, 2, 3], ["a", "b"]))` under CRuby.
#[allow(dead_code)]
pub const EXPECTED: &str = "[6,12,0,[2,3],[1,3],[1],[2,3,4],2,null,[1,1,2,2,3,3],[1,2],[1,2,3],3,1,null,null,[1,2],[1,2,3],[2,3],[],[2,3],[],[1,2,3],2,2,16,6,null,1,null,2,1,[0,2,6]]";

pub fn overlay() -> Overlay {
    emit_and_run::real_blog()
        .write("app/models/array_probe.rb", PROBE)
        .edit(
            "app/controllers/articles_controller.rb",
            "  def index\n",
            "  def index\n    @probe = ArrayProbe.run([1, 2, 3], [\"a\", \"b\"])\n",
        )
}
