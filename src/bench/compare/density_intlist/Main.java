package density_intlist;

import java.util.ArrayList;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        ArrayList<Integer> lst = new ArrayList<>(); // boxed: Java has no List<int>
        for (int i = 0; i < n; i++) lst.add(i);
        long s = 0;
        for (int i = 0; i < lst.size(); i++) s += lst.get(i);
        System.out.println("RESULT " + s);
    }
}
