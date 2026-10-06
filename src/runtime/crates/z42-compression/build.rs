//! cdylib identity for the published libz42_compression.{dylib,so} — same reason
//! as the `z42` crate's build.rs: rustc leaves the macOS install name at the
//! absolute build path (CI's `/Users/runner/work/…`) and sets no Linux SONAME.
//! z42vm dlopens this library by full path, so these only make the shipped file
//! relocatable and free of build-machine paths. The rlib ignores cdylib link args.

fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libz42_compression.dylib"),
        Ok("linux") => println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libz42_compression.so"),
        _ => {}
    }
}
