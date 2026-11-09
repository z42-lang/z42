fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // Public library identity is independent of this platform binding's Cargo target.
        println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libz42.so");
    }
}
