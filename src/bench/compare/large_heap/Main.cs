namespace large_heap;

sealed class LNode { public long Id; public long[] Data; public LNode Next; }

public static class Bench
{
    static LNode MakeChain(long first, int chain)
    {
        LNode head = null;
        for (int k = 0; k < chain; k++)
        {
            var n = new LNode { Id = first + k };
            var d = new long[8]; d[0] = n.Id; d[7] = n.Id * 3;
            n.Data = d; n.Next = head; head = n;
        }
        return head;
    }

    static long ChainSum(LNode h) { long s = 0; for (; h != null; h = h.Next) s += h.Data[0] + h.Data[7]; return s; }

    public static void Run(string[] args)
    {
        int slots = int.Parse(args[0]);
        int chain = 8; long churn = (long)slots * 8;
        var table = new LNode[slots];
        long nextId = 0;
        for (int s = 0; s < slots; s++) { table[s] = MakeChain(nextId, chain); nextId += chain; }
        long seed = 12345, acc = 0;
        for (long i = 0; i < churn; i++)
        {
            seed = (seed * 1103515245L + 12345L) % 2147483648L;
            int slot = (int)(seed % slots);
            acc += ChainSum(table[slot]);
            table[slot] = MakeChain(nextId, chain);
            nextId += chain;
            var t = new LNode { Id = i };
            var tmp = new long[4]; tmp[1] = i;
            acc += tmp[1] % 7 + t.Id % 5;
        }
        for (int s = 0; s < slots; s++) acc += ChainSum(table[s]);
        Console.WriteLine("RESULT " + acc);
    }
}
