package dict_ops;

import java.util.HashMap;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        HashMap<Integer, Integer> d = new HashMap<>();
        for (int i = 0; i < n; i++) d.put(i * 2, i);
        long acc = 0;
        for (int i = 0; i < 2 * n; i++) { Integer v = d.get(i); if (v != null) acc += v; }
        for (int i = 0; i < n; i += 2) d.put(i * 2, 1);
        acc += d.size() + d.get(0) + d.get(6);
        System.out.println("RESULT " + acc);
    }
}
