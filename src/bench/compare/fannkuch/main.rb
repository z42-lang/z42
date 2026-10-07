def fannkuch(n)
  perm1 = (0...n).to_a
  count = Array.new(n, 0)
  max_flips = checksum = perm_count = 0
  r = n
  loop do
    while r != 1
      count[r - 1] = r
      r -= 1
    end
    perm = perm1.dup
    flips = 0
    k = perm[0]
    while k != 0
      lo = 0
      hi = k
      while lo < hi
        perm[lo], perm[hi] = perm[hi], perm[lo]
        lo += 1
        hi -= 1
      end
      flips += 1
      k = perm[0]
    end
    max_flips = flips if flips > max_flips
    checksum += perm_count.even? ? flips : -flips
    loop do
      return [checksum, max_flips] if r == n
      p0 = perm1[0]
      i = 0
      while i < r
        perm1[i] = perm1[i + 1]
        i += 1
      end
      perm1[r] = p0
      count[r] -= 1
      break if count[r] > 0
      r += 1
    end
    perm_count += 1
  end
end

c, m = fannkuch(ARGV[0].to_i)
puts "RESULT #{c}:#{m}"
