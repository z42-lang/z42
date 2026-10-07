package parallel_sum;

public class Main {
    public static void main(String[] args) throws InterruptedException {
        long n = Long.parseLong(args[0]);
        int threads = 4;
        long[] partials = new long[threads];
        Thread[] ts = new Thread[threads];
        for (int t = 0; t < threads; t++) {
            final int id = t;
            ts[t] = new Thread(() -> {
                long chunk = n / threads;
                long lo = id * chunk, hi = id == threads - 1 ? n : lo + chunk;
                long acc = 0;
                for (long i = lo; i < hi; i++) acc += (i * i) % 7;
                partials[id] = acc;
            });
            ts[t].start();
        }
        long total = 0;
        for (int t = 0; t < threads; t++) { ts[t].join(); total += partials[t]; }
        System.out.println("RESULT " + total);
    }
}
