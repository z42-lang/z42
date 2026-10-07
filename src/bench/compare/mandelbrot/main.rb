size = ARGV[0].to_i
inside = 0
y = 0
while y < size
  ci = 2.0 * y / size - 1.0
  x = 0
  while x < size
    cr = 2.0 * x / size - 1.5
    zr = zi = 0.0
    escaped = false
    it = 0
    while it < 50
      tr = zr * zr - zi * zi + cr
      zi = 2.0 * zr * zi + ci
      zr = tr
      if zr * zr + zi * zi > 4.0
        escaped = true
        break
      end
      it += 1
    end
    inside += 1 unless escaped
    x += 1
  end
  y += 1
end
puts "RESULT #{inside}"
