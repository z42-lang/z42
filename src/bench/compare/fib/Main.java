package fib;

public class Main {
    static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

    public static void main(String[] args) {
        System.out.println("RESULT " + fib(Integer.parseInt(args[0])));
    }
}
