# Threads, as in every other language here. CRuby's GVL serialises them, so this
# measures the GVL, not parallel speedup.
N = ARGV[0].to_i
THREADS = 4
partials = Array.new(THREADS, 0)
threads = (0...THREADS).map do |t|
  Thread.new do
    chunk = N / THREADS
    lo = t * chunk
    hi = t == THREADS - 1 ? N : lo + chunk
    acc = 0
    i = lo
    while i < hi
      acc += (i * i) % 7
      i += 1
    end
    partials[t] = acc
  end
end
threads.each(&:join)
puts "RESULT #{partials.sum}"
