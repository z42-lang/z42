namespace nbody;

sealed class Body
{
    public double X, Y, Z, Vx, Vy, Vz, Mass;
    public Body(double x, double y, double z, double vx, double vy, double vz, double mass)
    {
        X = x; Y = y; Z = z;
        Vx = vx * Bench.Days; Vy = vy * Bench.Days; Vz = vz * Bench.Days;
        Mass = mass * Bench.SolarMass;
    }
}

public static class Bench
{
    public const double SolarMass = 39.47841760435743;
    public const double Days = 365.24;

    static Body[] MakeSystem()
    {
        Body[] b = {
            new Body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
            new Body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
                1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
            new Body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
                -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
            new Body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
                2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
            new Body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
                2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
        };
        double px = 0.0, py = 0.0, pz = 0.0;
        foreach (var o in b) { px = px + o.Vx * o.Mass; py = py + o.Vy * o.Mass; pz = pz + o.Vz * o.Mass; }
        b[0].Vx = -px / SolarMass; b[0].Vy = -py / SolarMass; b[0].Vz = -pz / SolarMass;
        return b;
    }

    static void Advance(Body[] b, double dt)
    {
        int n = b.Length;
        for (int i = 0; i < n; i++)
        {
            var bi = b[i];
            for (int j = i + 1; j < n; j++)
            {
                var bj = b[j];
                double dx = bi.X - bj.X, dy = bi.Y - bj.Y, dz = bi.Z - bj.Z;
                double d2 = dx * dx + dy * dy + dz * dz;
                double mag = dt / (d2 * Math.Sqrt(d2));
                double mi = bi.Mass * mag, mj = bj.Mass * mag;
                bi.Vx = bi.Vx - dx * mj; bi.Vy = bi.Vy - dy * mj; bi.Vz = bi.Vz - dz * mj;
                bj.Vx = bj.Vx + dx * mi; bj.Vy = bj.Vy + dy * mi; bj.Vz = bj.Vz + dz * mi;
            }
        }
        foreach (var bi in b) { bi.X = bi.X + dt * bi.Vx; bi.Y = bi.Y + dt * bi.Vy; bi.Z = bi.Z + dt * bi.Vz; }
    }

    static double Energy(Body[] b)
    {
        double e = 0.0;
        for (int i = 0; i < b.Length; i++)
        {
            var bi = b[i];
            e = e + 0.5 * bi.Mass * (bi.Vx * bi.Vx + bi.Vy * bi.Vy + bi.Vz * bi.Vz);
            for (int j = i + 1; j < b.Length; j++)
            {
                var bj = b[j];
                double dx = bi.X - bj.X, dy = bi.Y - bj.Y, dz = bi.Z - bj.Z;
                e = e - bi.Mass * bj.Mass / Math.Sqrt(dx * dx + dy * dy + dz * dz);
            }
        }
        return e;
    }

    public static void Run(string[] args)
    {
        var b = MakeSystem();
        int steps = int.Parse(args[0]);
        for (int s = 0; s < steps; s++) Advance(b, 0.01);
        Console.WriteLine("RESULT " + (long)(Energy(b) * 1000000000.0));
    }
}
