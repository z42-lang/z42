//! Thin native launcher: no Rust VM types cross the runtime-library boundary.
#[path = "cli/entry_protocol.rs"]
mod entry_protocol;
#[path = "pal/sys/launcher.rs"]
mod launcher;
fn main() {
    match launcher::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => { eprintln!("z42vm: {error:#}"); std::process::exit(1); }
    }
}
