import sys

n = int(sys.argv[1])
lst = []
for i in range(n):
    lst.append(i % 1000)
acc = 0
for p in range(10):
    for i in range(len(lst)):
        acc += lst[i] ^ p
print("RESULT %d" % acc)
