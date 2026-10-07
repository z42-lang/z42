// Dispatcher: `compare <workload> [args...]` runs `<workload>.Bench.Run(args)`.
using System.Reflection;

static class Program
{
    static int Main(string[] argv)
    {
        var type = typeof(Program).Assembly.GetType(argv[0] + ".Bench");
        if (type == null) { Console.Error.WriteLine("unknown workload: " + argv[0]); return 2; }
        var run = type.GetMethod("Run", BindingFlags.Public | BindingFlags.Static);
        run.Invoke(null, new object[] { argv[1..] });
        return 0;
    }
}
