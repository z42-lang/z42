n = ARGV[0].to_i
all = Array.new(n)
i = 0
while i < n
  a = Array.new(8, 0)
  a[0] = i
  a[7] = i * 3
  all[i] = a
  i += 1
end
puts "RESULT #{all.sum { |x| x[0] + x[7] }}"
