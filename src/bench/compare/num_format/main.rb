n = ARGV[0].to_i
acc = 0
i = 0
while i < n
  v = i * 7919 + 1
  s = v.to_s
  acc += s.length + s.to_i % 7
  i += 1
end
puts "RESULT #{acc}"
