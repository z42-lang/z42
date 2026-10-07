namespace num_format;

public static class Bench
{
    public static void Run(string[] args)
    {
        long n = long.Parse(args[0]);
        long acc = 0;
        for (long i = 0; i < n; i++)
        {
            long v = i * 7919 + 1;
            string s = v.ToString();
            acc += s.Length + long.Parse(s) % 7;
        }
        Console.WriteLine("RESULT " + acc);
    }
}
