package vcall_mono;

public class Main {
    static class Op { long step(long x) { return x; } }
    static class AddOne extends Op { @Override long step(long x) { return x + 1; } }

    static long drive(Op op, long n) { long acc = 0; for (long i = 0; i < n; i++) acc += op.step(i); return acc; }

    public static void main(String[] args) {
        System.out.println("RESULT " + drive(new AddOne(), Long.parseLong(args[0])));
    }
}
