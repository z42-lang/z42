def fib(n)
  n < 2 ? n : fib(n - 1) + fib(n - 2)
end

puts "RESULT #{fib(ARGV[0].to_i)}"
