class Node
  attr_reader :v, :nxt

  def initialize(v, nxt)
    @v = v
    @nxt = nxt
  end
end

n = ARGV[0].to_i
head = nil
i = 0
while i < n
  head = Node.new(i, head)
  i += 1
end
s = 0
p = head
while p
  s += p.v
  p = p.nxt
end
puts "RESULT #{s}"
