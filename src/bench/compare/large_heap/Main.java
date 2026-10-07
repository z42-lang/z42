package large_heap;

public class Main {
    static final class LNode { long id; long[] data; LNode next; }

    static LNode makeChain(long first, int chain) {
        LNode head = null;
        for (int k = 0; k < chain; k++) {
            LNode n = new LNode(); n.id = first + k;
            long[] d = new long[8]; d[0] = n.id; d[7] = n.id * 3;
            n.data = d; n.next = head; head = n;
        }
        return head;
    }

    static long chainSum(LNode h) { long s = 0; for (; h != null; h = h.next) s += h.data[0] + h.data[7]; return s; }

    public static void main(String[] args) {
        int slots = Integer.parseInt(args[0]);
        int chain = 8; long churn = (long) slots * 8;
        LNode[] table = new LNode[slots];
        long nextId = 0;
        for (int s = 0; s < slots; s++) { table[s] = makeChain(nextId, chain); nextId += chain; }
        long seed = 12345, acc = 0;
        for (long i = 0; i < churn; i++) {
            seed = (seed * 1103515245L + 12345L) % 2147483648L;
            int slot = (int) (seed % slots);
            acc += chainSum(table[slot]);
            table[slot] = makeChain(nextId, chain);
            nextId += chain;
            LNode t = new LNode(); t.id = i;
            long[] tmp = new long[4]; tmp[1] = i;
            acc += tmp[1] % 7 + t.id % 5;
        }
        for (int s = 0; s < slots; s++) acc += chainSum(table[s]);
        System.out.println("RESULT " + acc);
    }
}
