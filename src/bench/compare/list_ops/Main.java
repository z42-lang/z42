package list_ops;

import java.util.ArrayList;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        ArrayList<Integer> list = new ArrayList<>();
        for (int i = 0; i < n; i++) list.add(i % 1000);
        long acc = 0;
        for (int p = 0; p < 10; p++) for (int i = 0; i < list.size(); i++) acc += list.get(i) ^ p;
        System.out.println("RESULT " + acc);
    }
}
