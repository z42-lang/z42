package density_map;

import java.util.HashMap;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        HashMap<String, Integer> d = new HashMap<>();
        for (int i = 0; i < n; i++) d.put("k" + i, i);
        long s = d.size();
        for (int i = 0; i < n; i += 7919) s += d.get("k" + i);
        System.out.println("RESULT " + s);
    }
}
