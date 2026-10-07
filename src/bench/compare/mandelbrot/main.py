import sys

size = int(sys.argv[1])
inside = 0
for y in range(size):
    ci = 2.0 * y / size - 1.0
    for x in range(size):
        cr = 2.0 * x / size - 1.5
        zr = zi = 0.0
        escaped = False
        for _ in range(50):
            tr = zr * zr - zi * zi + cr
            zi = 2.0 * zr * zi + ci
            zr = tr
            if zr * zr + zi * zi > 4.0:
                escaped = True
                break
        if not escaped:
            inside += 1
print("RESULT %d" % inside)
