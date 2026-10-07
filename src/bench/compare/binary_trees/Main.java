package binary_trees;

public class Main {
    static final class TreeNode { TreeNode left, right; TreeNode(TreeNode l, TreeNode r) { left = l; right = r; } }

    static TreeNode bottom(int d) { return d > 0 ? new TreeNode(bottom(d - 1), bottom(d - 1)) : new TreeNode(null, null); }

    static int check(TreeNode t) { return t.left == null ? 1 : 1 + check(t.left) + check(t.right); }

    public static void main(String[] args) {
        int maxDepth = Math.max(6, Integer.parseInt(args[0]));
        int stretch = maxDepth + 1;
        System.out.println("stretch tree of depth " + stretch + "\t check: " + check(bottom(stretch)));
        TreeNode longLived = bottom(maxDepth);
        long total = 0;
        for (int d = 4; d <= maxDepth; d += 2) {
            int iters = 1 << (maxDepth - d + 4);
            int c = 0;
            for (int i = 0; i < iters; i++) c += check(bottom(d));
            System.out.println(iters + "\t trees of depth " + d + "\t check: " + c);
            total += c;
        }
        int ll = check(longLived);
        System.out.println("long lived tree of depth " + maxDepth + "\t check: " + ll);
        System.out.println("RESULT " + (total + ll));
    }
}
