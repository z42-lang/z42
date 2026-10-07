namespace dict_ops;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var d = new Dictionary<int, int>();
        for (int i = 0; i < n; i++) d[i * 2] = i;
        long acc = 0;
        for (int i = 0; i < 2 * n; i++) if (d.TryGetValue(i, out int v)) acc += v;
        for (int i = 0; i < n; i += 2) d[i * 2] = 1;
        acc += d.Count + d[0] + d[6];
        Console.WriteLine("RESULT " + acc);
    }
}
