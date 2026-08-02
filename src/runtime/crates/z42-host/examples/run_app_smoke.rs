//! Smoke reference for the embedded app-run path (add-embedded-app-run).
//!
//! A standalone native host that embeds the VM (links the z42 runtime via the
//! z42-host crate) and runs a z42 app.zpkg through `z42_host::run_app` — the
//! SAME entry the desktop self-contained shell + wasm/iOS/Android will use.
//! Contrast with the `z42vm` binary, which runs apps via `z42::app::run`
//! directly; this proves the embedding wrapper end-to-end.
//!
//! Usage:
//!   Z42_LIBS=<libs> cargo run -p z42-host --example run_app_smoke -- <app.zpkg> [prog args...]
//! e.g. run the test-agent over a compiled test module:
//!   ... -- z42.testagent.zpkg sample_tests.zbc json

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: run_app_smoke <app.zpkg> [prog args...]");
        std::process::exit(2);
    }
    let app = args[0].clone();
    let prog_args: Vec<String> = args[1..].to_vec(); // forwarded to the app's GetCommandLineArgs()
    let libs = std::env::var("Z42_LIBS").ok().map(PathBuf::from);

    // entry = None → use the app's baked-in entry; mode = None → build default.
    match z42_host::run_app(&app, None, libs, None, prog_args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("run_app error: {e:?}");
            std::process::exit(1);
        }
    }
}
