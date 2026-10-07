import sys

n = int(sys.argv[1])
d = {}
for i in range(n):
    d["k" + str(i)] = i
s = len(d)
for i in range(0, n, 7919):
    s += d["k" + str(i)]
print("RESULT %d" % s)
