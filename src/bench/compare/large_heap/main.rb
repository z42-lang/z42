class LNode
  attr_accessor :id, :data, :nxt

  def initialize
    @id = 0
    @data = nil
    @nxt = nil
  end
end

def make_chain(first, chain)
  head = nil
  k = 0
  while k < chain
    n = LNode.new
    n.id = first + k
    d = Array.new(8, 0)
    d[0] = n.id
    d[7] = n.id * 3
    n.data = d
    n.nxt = head
    head = n
    k += 1
  end
  head
end

def chain_sum(h)
  s = 0
  while h
    s += h.data[0] + h.data[7]
    h = h.nxt
  end
  s
end

def drive(slots)
  chain = 8
  table = Array.new(slots)
  next_id = 0
  s = 0
  while s < slots
    table[s] = make_chain(next_id, chain)
    next_id += chain
    s += 1
  end
  seed = 12345
  acc = 0
  i = 0
  while i < slots * 8
    seed = (seed * 1103515245 + 12345) % 2147483648
    slot = seed % slots
    acc += chain_sum(table[slot])
    table[slot] = make_chain(next_id, chain)
    next_id += chain
    t = LNode.new
    t.id = i
    tmp = Array.new(4, 0)
    tmp[1] = i
    acc += tmp[1] % 7 + t.id % 5
    i += 1
  end
  s = 0
  while s < slots
    acc += chain_sum(table[s])
    s += 1
  end
  acc
end

puts "RESULT #{drive(ARGV[0].to_i)}"
