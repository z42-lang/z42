using System.Text;

namespace str_builder;

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var sb = new StringBuilder();
        for (int i = 0; i < n; i++) sb.Append("item").Append(i.ToString()).Append(';');
        string s = sb.ToString();
        long sevens = 0;
        foreach (char c in s) if (c == '7') sevens++;
        Console.WriteLine("RESULT " + (s.Length + sevens));
    }
}
