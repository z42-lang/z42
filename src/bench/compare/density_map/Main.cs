namespace density_map;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var d = new Dictionary<string, int>();
        for (int i = 0; i < n; i++) d["k" + i.ToString()] = i;
        long s = d.Count;
        for (int i = 0; i < n; i += 7919) s += d["k" + i.ToString()];
        Console.WriteLine("RESULT " + s);
    }
}
