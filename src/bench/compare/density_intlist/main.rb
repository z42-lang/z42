n = ARGV[0].to_i
lst = []
i = 0
while i < n
  lst << i
  i += 1
end
puts "RESULT #{lst.sum}"
