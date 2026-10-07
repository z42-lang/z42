def apply(f, x)
  f.call(x)
end

n = ARGV[0].to_i
acc = 0
i = 0
while i < n
  k = i % 7
  f = ->(x) { x * 3 + k }
  acc += apply(f, i)
  i += 1
end
puts "RESULT #{acc}"
