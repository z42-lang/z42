class Op
  def step(x)
    x
  end
end

class AddOne < Op
  def step(x)
    x + 1
  end
end

def drive(op, n)
  acc = 0
  i = 0
  while i < n
    acc += op.step(i)
    i += 1
  end
  acc
end

puts "RESULT #{drive(AddOne.new, ARGV[0].to_i)}"
