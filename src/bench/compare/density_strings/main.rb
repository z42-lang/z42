n = ARGV[0].to_i
all = Array.new(n)
i = 0
while i < n
  all[i] = "k" + i.to_s
  i += 1
end
puts "RESULT #{all.sum(&:length)}"
