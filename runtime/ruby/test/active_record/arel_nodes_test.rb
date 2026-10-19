# frozen_string_literal: true
require_relative "../test_helper"
require "active_record/arel"

# Arel nodes render to the SQL text Relation interpolates. Binds are
# filled by Arel.sql: a node is embedded, a scalar is quoted.
class ArelNodesTest < Minitest::Test
  def test_sql_without_binds_is_the_fragment
    assert_equal "SUM(energy_kwh)", Arel.sql("SUM(energy_kwh)").to_s
    assert_instance_of Arel::Nodes::SqlLiteral, Arel.sql("SUM(energy_kwh)")
  end

  def test_named_bind_quotes_a_scalar_and_embeds_a_node
    quoted = Arel::Nodes.build_quoted("era's")
    assert_equal "'era''s'", quoted.to_s
    sql = Arel.sql("date < :boundary AND :flag", boundary: quoted, flag: true)
    assert_equal "date < 'era''s' AND 1", sql.to_s
  end

  def test_a_shorter_name_does_not_eat_a_longer_one
    sql = Arel.sql(":era_boundary > :era", era_boundary: 2, era: 1)
    assert_equal "2 > 1", sql.to_s
  end

  def test_a_placeholder_inside_a_string_literal_stays
    sql = Arel.sql("note = 'keep :name' AND name = :name", name: "Ada")
    assert_equal "note = 'keep :name' AND name = 'Ada'", sql.to_s
  end

  def test_positional_question_mark
    sql = Arel.sql("id = ? AND label = ?", 4, "a'b")
    assert_equal "id = 4 AND label = 'a''b'", sql.to_s
  end

  def test_not_case_and_named_function
    predicate = "active = 1"
    expr = Arel::Nodes::Not.new(predicate)
    assert_equal "NOT (active = 1)", expr.to_s
    cse = Arel::Nodes::Case.new.when(predicate).then(1).else(nil)
    assert_equal "CASE WHEN active = 1 THEN 1 ELSE NULL END", cse.to_s
    fn = Arel::Nodes::NamedFunction.new("COUNT", [cse])
    assert_equal "COUNT(CASE WHEN active = 1 THEN 1 ELSE NULL END)", fn.to_s
  end

  def test_a_sql_literal_bind_is_embedded_not_quoted
    inner = Arel.sql("lost_kwh > :n", n: 0)
    outer = Arel.sql("SUM(CASE WHEN :scope THEN 1 ELSE 0 END)", scope: inner)
    assert_equal "SUM(CASE WHEN lost_kwh > 0 THEN 1 ELSE 0 END)", outer.to_s
  end
end
