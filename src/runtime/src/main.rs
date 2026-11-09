//! Thin native launcher: no Rust VM types cross the runtime-library boundary.
#[cfg(feature = "component-loading")]
#[path = "host/entry_api.rs"]
mod entry_api;
#[cfg(feature = "component-loading")]
#[path = "pal/sys/launcher.rs"]
mod launcher;
#[cfg(feature = "component-loading")]
fn main() {
    match launcher::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => { eprintln!("z42vm: {error:#}"); std::process::exit(1); }
    }
}
#[cfg(not(feature = "component-loading"))]
fn main() {
    eprintln!("z42vm: dynamic launcher requires component-loading; use a static embedding on this target");
    std::process::exit(1);
}
