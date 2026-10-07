package closures;

import java.util.function.LongUnaryOperator;

public class Main {
    static long apply(LongUnaryOperator f, long x) { return f.applyAsLong(x); }

    public static void main(String[] args) {
        long n = Long.parseLong(args[0]);
        long acc = 0;
        for (long i = 0; i < n; i++) {
            long k = i % 7;
            LongUnaryOperator f = x -> x * 3 + k;
            acc += apply(f, i);
        }
        System.out.println("RESULT " + acc);
    }
}
