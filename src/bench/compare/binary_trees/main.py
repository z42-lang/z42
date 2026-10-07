import sys


class TreeNode:
    __slots__ = ("left", "right")

    def __init__(self, l, r):
        self.left = l
        self.right = r


def bottom(d):
    if d > 0:
        return TreeNode(bottom(d - 1), bottom(d - 1))
    return TreeNode(None, None)


def check(t):
    if t.left is None:
        return 1
    return 1 + check(t.left) + check(t.right)


max_depth = max(6, int(sys.argv[1]))
stretch = max_depth + 1
print("stretch tree of depth %d\t check: %d" % (stretch, check(bottom(stretch))))
long_lived = bottom(max_depth)
total = 0
for d in range(4, max_depth + 1, 2):
    iters = 1 << (max_depth - d + 4)
    c = 0
    for _ in range(iters):
        c += check(bottom(d))
    print("%d\t trees of depth %d\t check: %d" % (iters, d, c))
    total += c
ll = check(long_lived)
print("long lived tree of depth %d\t check: %d" % (max_depth, ll))
print("RESULT %d" % (total + ll))
