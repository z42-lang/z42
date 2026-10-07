package spectral_norm;

public class Main {
    static double a(int i, int j) { int ij = i + j; return 1.0 / (ij * (ij + 1) / 2 + i + 1); }

    static void mulAv(double[] v, double[] av, int n) {
        for (int i = 0; i < n; i++) { double s = 0.0; for (int j = 0; j < n; j++) s = s + a(i, j) * v[j]; av[i] = s; }
    }

    static void mulAtv(double[] v, double[] atv, int n) {
        for (int i = 0; i < n; i++) { double s = 0.0; for (int j = 0; j < n; j++) s = s + a(j, i) * v[j]; atv[i] = s; }
    }

    static void mulAtAv(double[] v, double[] dst, double[] tmp, int n) { mulAv(v, tmp, n); mulAtv(tmp, dst, n); }

    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        double[] u = new double[n], v = new double[n], tmp = new double[n];
        java.util.Arrays.fill(u, 1.0);
        for (int k = 0; k < 10; k++) { mulAtAv(u, v, tmp, n); mulAtAv(v, u, tmp, n); }
        double vbv = 0.0, vv = 0.0;
        for (int i = 0; i < n; i++) { vbv = vbv + u[i] * v[i]; vv = vv + v[i] * v[i]; }
        System.out.println("RESULT " + (long) (Math.sqrt(vbv / vv) * 1000000000.0));
    }
}
