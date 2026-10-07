n = ARGV[0].to_i
d = {}
i = 0
while i < n
  d["k" + i.to_s] = i
  i += 1
end
s = d.size
i = 0
while i < n
  s += d["k" + i.to_s]
  i += 7919
end
puts "RESULT #{s}"
