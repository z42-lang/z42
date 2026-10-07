namespace vcall_mono;

class Op { public virtual long Step(long x) => x; }
class AddOne : Op { public override long Step(long x) => x + 1; }

public static class Bench
{
    static long Drive(Op op, long n) { long acc = 0; for (long i = 0; i < n; i++) acc += op.Step(i); return acc; }

    public static void Run(string[] args) => Console.WriteLine("RESULT " + Drive(new AddOne(), long.Parse(args[0])));
}
