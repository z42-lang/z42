package str_split_join;

public class Main {
    public static void main(String[] args) {
        int rounds = Integer.parseInt(args[0]);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 200; i++) { if (i > 0) sb.append(","); sb.append("f").append(i * 37); }
        String line = sb.toString();
        long acc = 0;
        for (int r = 0; r < rounds; r++) {
            String[] parts = line.split(",");
            String joined = String.join(";", parts);
            acc += joined.length() + parts.length;
        }
        System.out.println("RESULT " + acc);
    }
}
