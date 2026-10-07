n = ARGV[0].to_i
s = String.new
i = 0
while i < n
  s << "item" << i.to_s << ";"
  i += 1
end
puts "RESULT #{s.length + s.count('7')}"
