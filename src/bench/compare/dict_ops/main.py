import sys

n = int(sys.argv[1])
d = {}
for i in range(n):
    d[i * 2] = i
acc = 0
for i in range(2 * n):
    if i in d:
        acc += d[i]
for i in range(0, n, 2):
    d[i * 2] = 1
acc += len(d) + d[0] + d[6]
print("RESULT %d" % acc)
