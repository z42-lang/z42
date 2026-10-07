namespace spectral_norm;

public static class Bench
{
    static double A(int i, int j) { int ij = i + j; return 1.0 / (ij * (ij + 1) / 2 + i + 1); }

    static void MulAv(double[] v, double[] av, int n)
    {
        for (int i = 0; i < n; i++) { double s = 0.0; for (int j = 0; j < n; j++) s = s + A(i, j) * v[j]; av[i] = s; }
    }

    static void MulAtv(double[] v, double[] atv, int n)
    {
        for (int i = 0; i < n; i++) { double s = 0.0; for (int j = 0; j < n; j++) s = s + A(j, i) * v[j]; atv[i] = s; }
    }

    static void MulAtAv(double[] v, double[] dst, double[] tmp, int n) { MulAv(v, tmp, n); MulAtv(tmp, dst, n); }

    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        double[] u = new double[n], v = new double[n], tmp = new double[n];
        Array.Fill(u, 1.0);
        for (int k = 0; k < 10; k++) { MulAtAv(u, v, tmp, n); MulAtAv(v, u, tmp, n); }
        double vbv = 0.0, vv = 0.0;
        for (int i = 0; i < n; i++) { vbv = vbv + u[i] * v[i]; vv = vv + v[i] * v[i]; }
        Console.WriteLine("RESULT " + (long)(Math.Sqrt(vbv / vv) * 1000000000.0));
    }
}
