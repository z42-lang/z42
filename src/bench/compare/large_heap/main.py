import sys


class LNode:
    __slots__ = ("id", "data", "next")

    def __init__(self):
        self.id = 0
        self.data = None
        self.next = None


def make_chain(first, chain):
    head = None
    for k in range(chain):
        n = LNode()
        n.id = first + k
        d = [0] * 8
        d[0] = n.id
        d[7] = n.id * 3
        n.data = d
        n.next = head
        head = n
    return head


def chain_sum(h):
    s = 0
    while h is not None:
        s += h.data[0] + h.data[7]
        h = h.next
    return s


def drive(slots):
    chain = 8
    table = [None] * slots
    next_id = 0
    for s in range(slots):
        table[s] = make_chain(next_id, chain)
        next_id += chain
    seed = 12345
    acc = 0
    for i in range(slots * 8):
        seed = (seed * 1103515245 + 12345) % 2147483648
        slot = seed % slots
        acc += chain_sum(table[slot])
        table[slot] = make_chain(next_id, chain)
        next_id += chain
        t = LNode()
        t.id = i
        tmp = [0] * 4
        tmp[1] = i
        acc += tmp[1] % 7 + t.id % 5
    for s in range(slots):
        acc += chain_sum(table[s])
    return acc


print("RESULT %d" % drive(int(sys.argv[1])))
