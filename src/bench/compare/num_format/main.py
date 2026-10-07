import sys

n = int(sys.argv[1])
acc = 0
for i in range(n):
    v = i * 7919 + 1
    s = str(v)
    acc += len(s) + int(s) % 7
print("RESULT %d" % acc)
