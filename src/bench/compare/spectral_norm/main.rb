def a(i, j)
  ij = i + j
  1.0 / (ij * (ij + 1) / 2 + i + 1)
end

def mul_av(v, av, n)
  i = 0
  while i < n
    s = 0.0
    j = 0
    while j < n
      s = s + a(i, j) * v[j]
      j += 1
    end
    av[i] = s
    i += 1
  end
end

def mul_atv(v, atv, n)
  i = 0
  while i < n
    s = 0.0
    j = 0
    while j < n
      s = s + a(j, i) * v[j]
      j += 1
    end
    atv[i] = s
    i += 1
  end
end

def mul_atav(v, dst, tmp, n)
  mul_av(v, tmp, n)
  mul_atv(tmp, dst, n)
end

n = ARGV[0].to_i
u = Array.new(n, 1.0)
v = Array.new(n, 0.0)
tmp = Array.new(n, 0.0)
10.times do
  mul_atav(u, v, tmp, n)
  mul_atav(v, u, tmp, n)
end
vbv = vv = 0.0
n.times do |i|
  vbv = vbv + u[i] * v[i]
  vv = vv + v[i] * v[i]
end
puts "RESULT #{(Math.sqrt(vbv / vv) * 1000000000.0).to_i}"
