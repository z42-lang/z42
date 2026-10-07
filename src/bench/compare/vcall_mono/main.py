import sys


class Op:
    def step(self, x):
        return x


class AddOne(Op):
    def step(self, x):
        return x + 1


def drive(op, n):
    acc = 0
    for i in range(n):
        acc += op.step(i)
    return acc


print("RESULT %d" % drive(AddOne(), int(sys.argv[1])))
