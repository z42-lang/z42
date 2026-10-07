package num_format;

public class Main {
    public static void main(String[] args) {
        long n = Long.parseLong(args[0]);
        long acc = 0;
        for (long i = 0; i < n; i++) {
            long v = i * 7919 + 1;
            String s = Long.toString(v);
            acc += s.length() + Long.parseLong(s) % 7;
        }
        System.out.println("RESULT " + acc);
    }
}
