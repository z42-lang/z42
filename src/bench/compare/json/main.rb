require 'json'

def record(i)
  { 'id' => i, 'name' => "user#{i}", 'active' => (i % 3).zero?,
    'tags' => ["t#{i % 10}", 'x'], 'pos' => { 'x' => i % 100, 'y' => i % 37 } }
end

n = ARGV[0].to_i
root = (0...n).map { |i| record(i) }
text = JSON.generate(root)
back = JSON.parse(text)
acc = text.length
back.each do |r|
  acc += r['id'] + r['pos']['y']
  acc += 1 if r['active']
end
puts "RESULT #{acc}"
