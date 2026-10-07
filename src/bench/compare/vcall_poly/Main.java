package vcall_poly;

public class Main {
    static class Shape { long area(long k) { return 0; } }
    static class Square extends Shape { @Override long area(long k) { return k * k; } }
    static class Rect extends Shape { @Override long area(long k) { return k * 2; } }
    static class Tri extends Shape { @Override long area(long k) { return k / 2; } }
    static class Dot extends Shape { @Override long area(long k) { return 1; } }

    static long drive(Shape[] shapes, long n) {
        long acc = 0;
        for (long i = 0; i < n; i++) acc += shapes[(int) (i % 4)].area(i % 1000);
        return acc;
    }

    public static void main(String[] args) {
        Shape[] shapes = { new Square(), new Rect(), new Tri(), new Dot() };
        System.out.println("RESULT " + drive(shapes, Long.parseLong(args[0])));
    }
}
