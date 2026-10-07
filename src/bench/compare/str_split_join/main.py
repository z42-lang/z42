import sys

rounds = int(sys.argv[1])
line = ",".join("f" + str(i * 37) for i in range(200))
acc = 0
for _ in range(rounds):
    parts = line.split(",")
    joined = ";".join(parts)
    acc += len(joined) + len(parts)
print("RESULT %d" % acc)
