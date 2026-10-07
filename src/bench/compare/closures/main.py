import sys


def apply(f, x):
    return f(x)


def main(n):
    acc = 0
    for i in range(n):
        k = i % 7
        f = lambda x, k=k: x * 3 + k
        acc += apply(f, i)
    return acc


print("RESULT %d" % main(int(sys.argv[1])))
