//! Thin native launcher: no Rust VM types cross the runtime-library boundary.
#[cfg(all(feature = "component-loading", feature = "cli"))]
#[path = "host/entry_api.rs"]
mod entry_api;
#[cfg(all(feature = "component-loading", feature = "cli"))]
#[path = "pal/sys/launcher.rs"]
mod launcher;
#[cfg(all(feature = "component-loading", feature = "cli"))]
fn main() {
    match launcher::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => { eprintln!("z42vm: {error:#}"); std::process::exit(1); }
    }
}
#[cfg(not(all(feature = "component-loading", feature = "cli")))]
fn main() {
    eprintln!("z42vm: dynamic launcher requires cli and component-loading; use a static embedding on this target");
    std::process::exit(1);
}
