n = ARGV[0].to_i
d = {}
i = 0
while i < n
  d[i * 2] = i
  i += 1
end
acc = 0
i = 0
while i < 2 * n
  v = d[i]
  acc += v if v
  i += 1
end
i = 0
while i < n
  d[i * 2] = 1
  i += 2
end
acc += d.size + d[0] + d[6]
puts "RESULT #{acc}"
