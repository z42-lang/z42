fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args, ["space value", "中文", "--value=quoted\"value"]);
    std::process::exit(37);
}
