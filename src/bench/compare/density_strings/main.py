import sys

n = int(sys.argv[1])
allstr = [None] * n
for i in range(n):
    allstr[i] = "k" + str(i)
print("RESULT %d" % sum(len(x) for x in allstr))
