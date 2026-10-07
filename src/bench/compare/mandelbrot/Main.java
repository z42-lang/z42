package mandelbrot;

public class Main {
    public static void main(String[] args) {
        int size = Integer.parseInt(args[0]);
        long inside = 0;
        for (int y = 0; y < size; y++) {
            double ci = 2.0 * y / size - 1.0;
            for (int x = 0; x < size; x++) {
                double cr = 2.0 * x / size - 1.5;
                double zr = 0.0, zi = 0.0;
                boolean escaped = false;
                for (int it = 0; it < 50; it++) {
                    double tr = zr * zr - zi * zi + cr;
                    zi = 2.0 * zr * zi + ci;
                    zr = tr;
                    if (zr * zr + zi * zi > 4.0) { escaped = true; break; }
                }
                if (!escaped) inside++;
            }
        }
        System.out.println("RESULT " + inside);
    }
}
