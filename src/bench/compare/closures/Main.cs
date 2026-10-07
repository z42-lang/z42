namespace closures;

public static class Bench
{
    static long Apply(Func<long, long> f, long x) => f(x);

    public static void Run(string[] args)
    {
        long n = long.Parse(args[0]);
        long acc = 0;
        for (long i = 0; i < n; i++)
        {
            long k = i % 7;
            Func<long, long> f = x => x * 3 + k;
            acc += Apply(f, i);
        }
        Console.WriteLine("RESULT " + acc);
    }
}
