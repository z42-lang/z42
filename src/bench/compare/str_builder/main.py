import sys

n = int(sys.argv[1])
parts = []
for i in range(n):
    parts.append("item")
    parts.append(str(i))
    parts.append(";")
s = "".join(parts)
print("RESULT %d" % (len(s) + s.count("7")))
