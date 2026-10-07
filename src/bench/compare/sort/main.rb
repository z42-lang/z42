n = ARGV[0].to_i
a = []
seed = 42
n.times do
  seed = (seed * 1103515245 + 12345) % 2147483648
  a << seed % 1000000
end
a.sort!
acc = 0
i = 0
while i < n
  acc += a[i] * (i % 1000)
  i += 1
end
puts "RESULT #{acc}"
