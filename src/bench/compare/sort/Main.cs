namespace sort;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var a = new List<int>();
        long seed = 42;
        for (int i = 0; i < n; i++)
        {
            seed = (seed * 1103515245L + 12345L) % 2147483648L;
            a.Add((int)(seed % 1000000));
        }
        a.Sort();
        long acc = 0;
        for (int i = 0; i < n; i++) acc += (long)a[i] * (i % 1000);
        Console.WriteLine("RESULT " + acc);
    }
}
