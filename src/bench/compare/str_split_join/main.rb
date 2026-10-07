rounds = ARGV[0].to_i
line = (0...200).map { |i| "f#{i * 37}" }.join(",")
acc = 0
rounds.times do
  parts = line.split(",")
  joined = parts.join(";")
  acc += joined.length + parts.length
end
puts "RESULT #{acc}"
