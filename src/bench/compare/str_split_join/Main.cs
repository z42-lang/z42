using System.Text;

namespace str_split_join;

public static class Bench
{
    public static void Run(string[] args)
    {
        int rounds = int.Parse(args[0]);
        var sb = new StringBuilder();
        for (int i = 0; i < 200; i++) { if (i > 0) sb.Append(','); sb.Append('f').Append(i * 37); }
        string line = sb.ToString();
        long acc = 0;
        for (int r = 0; r < rounds; r++)
        {
            string[] parts = line.Split(",");
            string joined = string.Join(";", parts);
            acc += joined.Length + parts.Length;
        }
        Console.WriteLine("RESULT " + acc);
    }
}
