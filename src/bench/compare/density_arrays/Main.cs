namespace density_arrays;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var all = new long[n][];
        for (int i = 0; i < n; i++) { var a = new long[8]; a[0] = i; a[7] = (long)i * 3; all[i] = a; }
        long s = 0;
        for (int i = 0; i < n; i++) s += all[i][0] + all[i][7];
        Console.WriteLine("RESULT " + s);
    }
}
