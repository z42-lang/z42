namespace vcall_poly;

class Shape { public virtual long Area(long k) => 0; }
class Square : Shape { public override long Area(long k) => k * k; }
class Rect : Shape { public override long Area(long k) => k * 2; }
class Tri : Shape { public override long Area(long k) => k / 2; }
class Dot : Shape { public override long Area(long k) => 1; }

public static class Bench
{
    static long Drive(Shape[] shapes, long n)
    {
        long acc = 0;
        for (long i = 0; i < n; i++) acc += shapes[(int)(i % 4)].Area(i % 1000);
        return acc;
    }

    public static void Run(string[] args)
    {
        Shape[] shapes = { new Square(), new Rect(), new Tri(), new Dot() };
        Console.WriteLine("RESULT " + Drive(shapes, long.Parse(args[0])));
    }
}
