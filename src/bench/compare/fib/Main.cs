namespace fib;

public static class Bench
{
    static int Fib(int n) => n < 2 ? n : Fib(n - 1) + Fib(n - 2);

    public static void Run(string[] args) => Console.WriteLine("RESULT " + Fib(int.Parse(args[0])));
}
