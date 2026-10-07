namespace list_ops;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var list = new List<int>();
        for (int i = 0; i < n; i++) list.Add(i % 1000);
        long acc = 0;
        for (int p = 0; p < 10; p++) for (int i = 0; i < list.Count; i++) acc += list[i] ^ p;
        Console.WriteLine("RESULT " + acc);
    }
}
