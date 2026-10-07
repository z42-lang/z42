class TreeNode
  attr_reader :left, :right

  def initialize(l, r)
    @left = l
    @right = r
  end
end

def bottom(d)
  d > 0 ? TreeNode.new(bottom(d - 1), bottom(d - 1)) : TreeNode.new(nil, nil)
end

def check(t)
  t.left.nil? ? 1 : 1 + check(t.left) + check(t.right)
end

max_depth = [6, ARGV[0].to_i].max
stretch = max_depth + 1
puts "stretch tree of depth #{stretch}\t check: #{check(bottom(stretch))}"
long_lived = bottom(max_depth)
total = 0
4.step(max_depth, 2) do |d|
  iters = 1 << (max_depth - d + 4)
  c = 0
  iters.times { c += check(bottom(d)) }
  puts "#{iters}\t trees of depth #{d}\t check: #{c}"
  total += c
end
ll = check(long_lived)
puts "long lived tree of depth #{max_depth}\t check: #{ll}"
puts "RESULT #{total + ll}"
