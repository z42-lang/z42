package density_strings;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        String[] all = new String[n];
        for (int i = 0; i < n; i++) all[i] = "k" + i;
        long s = 0;
        for (int i = 0; i < n; i++) s += all[i].length();
        System.out.println("RESULT " + s);
    }
}
