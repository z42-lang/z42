import math
import sys

SOLAR_MASS = 39.47841760435743
DAYS = 365.24


class Body:
    __slots__ = ("x", "y", "z", "vx", "vy", "vz", "mass")

    def __init__(self, x, y, z, vx, vy, vz, mass):
        self.x, self.y, self.z = x, y, z
        self.vx, self.vy, self.vz = vx * DAYS, vy * DAYS, vz * DAYS
        self.mass = mass * SOLAR_MASS


def make_system():
    b = [
        Body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
        Body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
             1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
        Body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
             -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
        Body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
             2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
        Body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
             2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
    ]
    px = py = pz = 0.0
    for o in b:
        px = px + o.vx * o.mass
        py = py + o.vy * o.mass
        pz = pz + o.vz * o.mass
    b[0].vx = -px / SOLAR_MASS
    b[0].vy = -py / SOLAR_MASS
    b[0].vz = -pz / SOLAR_MASS
    return b


def advance(b, dt):
    n = len(b)
    for i in range(n):
        bi = b[i]
        for j in range(i + 1, n):
            bj = b[j]
            dx = bi.x - bj.x
            dy = bi.y - bj.y
            dz = bi.z - bj.z
            d2 = dx * dx + dy * dy + dz * dz
            mag = dt / (d2 * math.sqrt(d2))
            mi = bi.mass * mag
            mj = bj.mass * mag
            bi.vx = bi.vx - dx * mj
            bi.vy = bi.vy - dy * mj
            bi.vz = bi.vz - dz * mj
            bj.vx = bj.vx + dx * mi
            bj.vy = bj.vy + dy * mi
            bj.vz = bj.vz + dz * mi
    for bi in b:
        bi.x = bi.x + dt * bi.vx
        bi.y = bi.y + dt * bi.vy
        bi.z = bi.z + dt * bi.vz


def energy(b):
    e = 0.0
    n = len(b)
    for i in range(n):
        bi = b[i]
        e = e + 0.5 * bi.mass * (bi.vx * bi.vx + bi.vy * bi.vy + bi.vz * bi.vz)
        for j in range(i + 1, n):
            bj = b[j]
            dx = bi.x - bj.x
            dy = bi.y - bj.y
            dz = bi.z - bj.z
            e = e - bi.mass * bj.mass / math.sqrt(dx * dx + dy * dy + dz * dz)
    return e


b = make_system()
for _ in range(int(sys.argv[1])):
    advance(b, 0.01)
print("RESULT %d" % int(energy(b) * 1000000000.0))
