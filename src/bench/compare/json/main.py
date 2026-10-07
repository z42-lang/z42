import json
import sys


def record(i):
    return {"id": i, "name": "user" + str(i), "active": i % 3 == 0,
            "tags": ["t" + str(i % 10), "x"], "pos": {"x": i % 100, "y": i % 37}}


n = int(sys.argv[1])
root = [record(i) for i in range(n)]
text = json.dumps(root, separators=(",", ":"))
back = json.loads(text)
acc = len(text)
for r in back:
    acc += r["id"] + r["pos"]["y"]
    if r["active"]:
        acc += 1
print("RESULT %d" % acc)
