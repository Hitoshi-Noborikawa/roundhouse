class Worker
  include Dry::Monads[:result]

  def early(q)
    yield check(q)
  end

  include Dry::Monads::Do.for(:work)

  def work(q)
    v = yield check(q)
    "worked #{v}"
  rescue Dry::Monads::Do::Halt => e
    "halted #{e.result.failure}"
  end

  def other(q)
    yield check(q)
  end

  def check(q)
    q.positive? ? Success(q) : Failure(:negative)
  end
end

class ChildWorker < Worker
  def work(q)
    yield check(q)
  end
end
