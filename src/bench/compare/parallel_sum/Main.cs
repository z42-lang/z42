namespace parallel_sum;

public static class Bench
{
    public static void Run(string[] args)
    {
        long n = long.Parse(args[0]);
        const int threads = 4;
        var partials = new long[threads];
        var ts = new Thread[threads];
        for (int t = 0; t < threads; t++)
        {
            int id = t;
            ts[t] = new Thread(() =>
            {
                long chunk = n / threads;
                long lo = id * chunk, hi = id == threads - 1 ? n : lo + chunk;
                long acc = 0;
                for (long i = lo; i < hi; i++) acc += (i * i) % 7;
                partials[id] = acc;
            });
            ts[t].Start();
        }
        long total = 0;
        for (int t = 0; t < threads; t++) { ts[t].Join(); total += partials[t]; }
        Console.WriteLine("RESULT " + total);
    }
}
