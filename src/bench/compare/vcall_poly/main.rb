class Shape
  def area(_k)
    0
  end
end

class Square < Shape
  def area(k)
    k * k
  end
end

class Rect < Shape
  def area(k)
    k * 2
  end
end

class Tri < Shape
  def area(k)
    k / 2
  end
end

class Dot < Shape
  def area(_k)
    1
  end
end

def drive(shapes, n)
  acc = 0
  i = 0
  while i < n
    acc += shapes[i % 4].area(i % 1000)
    i += 1
  end
  acc
end

puts "RESULT #{drive([Square.new, Rect.new, Tri.new, Dot.new], ARGV[0].to_i)}"
