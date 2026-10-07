namespace density_objects;

sealed class Node { public int V; public Node Next; public Node(int v, Node n) { V = v; Next = n; } }

public static class Bench
{
    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        Node head = null;
        for (int i = 0; i < n; i++) head = new Node(i, head);
        long s = 0;
        for (var p = head; p != null; p = p.Next) s += p.V;
        Console.WriteLine("RESULT " + s);
    }
}
