//! Process policy for REPL editor library paths; evaluated only on first load.
use std::path::PathBuf;

pub(crate) fn candidates() -> Vec<PathBuf> {
    let file = crate::pal::dynlib::file_name("z42_repl");
    let mut out = Vec::new();
    if let Some(over) = crate::config::runtime_config().repl_native.as_ref() {
        let p = over.clone();
        // Accept either a direct file path or a directory containing the lib.
        out.push(p.join(&file));
        out.push(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(&file));
            // <sdk>/programs/z42i/ (beside the interactive payload), via the
            // shared colocated-native-dependency resolver.
            if let Some(sdk_root) = dir.parent() {
                let z42i_dir = sdk_root.join("programs").join("z42i");
                if let Some(p) = crate::pal::dynlib::resolve_native_beside(&z42i_dir, "z42_repl") {
                    out.push(p);
                }
            }
        }
    }
    out
}
