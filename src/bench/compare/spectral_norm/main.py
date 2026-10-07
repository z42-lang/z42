import math
import sys


def a(i, j):
    ij = i + j
    return 1.0 / (ij * (ij + 1) // 2 + i + 1)


def mul_av(v, av, n):
    for i in range(n):
        s = 0.0
        for j in range(n):
            s = s + a(i, j) * v[j]
        av[i] = s


def mul_atv(v, atv, n):
    for i in range(n):
        s = 0.0
        for j in range(n):
            s = s + a(j, i) * v[j]
        atv[i] = s


def mul_atav(v, dst, tmp, n):
    mul_av(v, tmp, n)
    mul_atv(tmp, dst, n)


n = int(sys.argv[1])
u = [1.0] * n
v = [0.0] * n
tmp = [0.0] * n
for _ in range(10):
    mul_atav(u, v, tmp, n)
    mul_atav(v, u, tmp, n)
vbv = vv = 0.0
for i in range(n):
    vbv = vbv + u[i] * v[i]
    vv = vv + v[i] * v[i]
print("RESULT %d" % int(math.sqrt(vbv / vv) * 1000000000.0))
