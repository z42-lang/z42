import sys

n = int(sys.argv[1])
allarr = [None] * n
for i in range(n):
    a = [0] * 8
    a[0] = i
    a[7] = i * 3
    allarr[i] = a
print("RESULT %d" % sum(a[0] + a[7] for a in allarr))
