import sys

n = int(sys.argv[1])
lst = []
for i in range(n):
    lst.append(i)
print("RESULT %d" % sum(lst))
