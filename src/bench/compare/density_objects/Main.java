package density_objects;

public class Main {
    static final class Node { int v; Node next; Node(int v, Node n) { this.v = v; this.next = n; } }

    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        Node head = null;
        for (int i = 0; i < n; i++) head = new Node(i, head);
        long s = 0;
        for (Node p = head; p != null; p = p.next) s += p.v;
        System.out.println("RESULT " + s);
    }
}
