import sys


class Node:
    __slots__ = ("v", "next")

    def __init__(self, v, nxt):
        self.v = v
        self.next = nxt


n = int(sys.argv[1])
head = None
for i in range(n):
    head = Node(i, head)
s = 0
p = head
while p is not None:
    s += p.v
    p = p.next
print("RESULT %d" % s)
