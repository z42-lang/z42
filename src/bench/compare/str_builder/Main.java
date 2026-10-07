package str_builder;

public class Main {
    public static void main(String[] args) {
        int n = Integer.parseInt(args[0]);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < n; i++) sb.append("item").append(Integer.toString(i)).append(";");
        String s = sb.toString();
        long sevens = 0;
        for (int i = 0; i < s.length(); i++) if (s.charAt(i) == '7') sevens++;
        System.out.println("RESULT " + (s.length() + sevens));
    }
}
