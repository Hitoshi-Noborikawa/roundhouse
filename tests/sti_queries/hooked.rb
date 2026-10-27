check("the hook runs on a subclass's row", Article.find_by(title: "f").hooked, true)
check("the base builds a subclass's row as it", Article.find_by(title: "f").class.name, "Feature")
puts "hooked sti queries passed"
