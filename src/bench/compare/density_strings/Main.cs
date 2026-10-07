namespace density_strings;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var all = new string[n];
        for (int i = 0; i < n; i++) all[i] = "k" + i.ToString();
        long s = 0;
        for (int i = 0; i < n; i++) s += all[i].Length;
        Console.WriteLine("RESULT " + s);
    }
}
