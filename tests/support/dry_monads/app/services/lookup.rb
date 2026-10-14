class Lookup
  include Dry::Monads[:result]

  def find(key)
    key == :a ? Success(1) : Failure(:missing)
  end
end
