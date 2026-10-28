# activerecord 8.1.4: what tests/record_equality_new.rs checks.
#
#     ruby tests/record_equality_new/oracle.rb
require "active_record"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define { create_table(:articles) { |t| t.string :title; t.text :body } }
class Article < ActiveRecord::Base; end

def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a = Article.new(title: "t")
b = Article.new(title: "t")
check("two new records", [a == b, a.eql?(b), [a, b].uniq.size], [false, false, 2])
check("a new record is itself", a == a, true)
s = Article.create!(title: "s", body: "a body long enough to pass")
l1 = Article.find(s.id)
l2 = Article.find(s.id)
check("two loads of one row", [l1 == l2, [l1, l2].uniq.size, l1 == s], [true, 1, true])
check("a new record and a loaded one", a == l1, false)
s.destroy
check("a destroyed record and a load of its row", [s == l1, [s, l1].uniq.size], [true, 1])
puts "record equality passed"
