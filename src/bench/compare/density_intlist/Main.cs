namespace density_intlist;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var lst = new List<int>();
        for (int i = 0; i < n; i++) lst.Add(i);
        long s = 0;
        for (int i = 0; i < lst.Count; i++) s += lst[i];
        Console.WriteLine("RESULT " + s);
    }
}
