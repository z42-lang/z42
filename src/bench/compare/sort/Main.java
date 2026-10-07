package sort;

import java.util.ArrayList;
import java.util.Collections;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        ArrayList<Integer> a = new ArrayList<>(); // List<int> equivalent (boxed), sorted by the stdlib
        long seed = 42;
        for (int i = 0; i < n; i++) {
            seed = (seed * 1103515245L + 12345L) % 2147483648L;
            a.add((int) (seed % 1000000));
        }
        Collections.sort(a);
        long acc = 0;
        for (int i = 0; i < n; i++) acc += (long) a.get(i) * (i % 1000);
        System.out.println("RESULT " + acc);
    }
}
