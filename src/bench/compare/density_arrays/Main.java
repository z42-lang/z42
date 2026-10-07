package density_arrays;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        long[][] all = new long[n][];
        for (int i = 0; i < n; i++) { long[] a = new long[8]; a[0] = i; a[7] = (long) i * 3; all[i] = a; }
        long s = 0;
        for (int i = 0; i < n; i++) s += all[i][0] + all[i][7];
        System.out.println("RESULT " + s);
    }
}
