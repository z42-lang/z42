n = ARGV[0].to_i
list = []
i = 0
while i < n
  list << i % 1000
  i += 1
end
acc = 0
10.times do |p|
  i = 0
  len = list.size
  while i < len
    acc += list[i] ^ p
    i += 1
  end
end
puts "RESULT #{acc}"
