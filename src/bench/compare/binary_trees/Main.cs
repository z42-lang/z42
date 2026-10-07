namespace binary_trees;

sealed class TreeNode { public TreeNode Left, Right; public TreeNode(TreeNode l, TreeNode r) { Left = l; Right = r; } }

public static class Bench
{
    static TreeNode Bottom(int d) => d > 0 ? new TreeNode(Bottom(d - 1), Bottom(d - 1)) : new TreeNode(null, null);
    static int Check(TreeNode t) => t.Left == null ? 1 : 1 + Check(t.Left) + Check(t.Right);

    public static void Run(string[] args)
    {
        int maxDepth = Math.Max(6, int.Parse(args[0]));
        int stretch = maxDepth + 1;
        Console.WriteLine($"stretch tree of depth {stretch}\t check: {Check(Bottom(stretch))}");
        var longLived = Bottom(maxDepth);
        long total = 0;
        for (int d = 4; d <= maxDepth; d += 2)
        {
            int iters = 1 << (maxDepth - d + 4);
            int c = 0;
            for (int i = 0; i < iters; i++) c += Check(Bottom(d));
            Console.WriteLine($"{iters}\t trees of depth {d}\t check: {c}");
            total += c;
        }
        int ll = Check(longLived);
        Console.WriteLine($"long lived tree of depth {maxDepth}\t check: {ll}");
        Console.WriteLine("RESULT " + (total + ll));
    }
}
