import sys

n = int(sys.argv[1])
a = []
seed = 42
for _ in range(n):
    seed = (seed * 1103515245 + 12345) % 2147483648
    a.append(seed % 1000000)
a.sort()
acc = 0
for i in range(n):
    acc += a[i] * (i % 1000)
print("RESULT %d" % acc)
