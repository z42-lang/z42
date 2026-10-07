import sys


class Shape:
    def area(self, k):
        return 0


class Square(Shape):
    def area(self, k):
        return k * k


class Rect(Shape):
    def area(self, k):
        return k * 2


class Tri(Shape):
    def area(self, k):
        return k // 2


class Dot(Shape):
    def area(self, k):
        return 1


def drive(shapes, n):
    acc = 0
    for i in range(n):
        acc += shapes[i % 4].area(i % 1000)
    return acc


print("RESULT %d" % drive([Square(), Rect(), Tri(), Dot()], int(sys.argv[1])))
