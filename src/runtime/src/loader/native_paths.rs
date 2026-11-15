//! Shared native search policy for components and independent runtime libraries.
use std::path::PathBuf;
static IMAGE_ANCHOR: u8 = 0;

pub(crate) fn directories() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Explicit paths retain priority over executable- and core-relative defaults.
    paths.extend(crate::config::runtime_config_snapshot().native_search_paths.iter().cloned());

    // Support SDK bin/native siblings and colocated development libraries.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent_dir) = exe.parent() {
            if let Some(sdk_root) = parent_dir.parent() {
                paths.push(sdk_root.join("native"));
            }
            paths.push(parent_dir.join("native"));
            paths.push(parent_dir.to_path_buf());
        }
    }

    // Embedded libraries need not live beside the application's executable.
    // Keep explicit and executable-relative overrides ahead of the core image.
    if let Some(image) = crate::pal::dynlib::image_path(core::ptr::addr_of!(IMAGE_ANCHOR).cast()) {
        if let Some(directory) = image.parent() {
            let directory = directory.to_path_buf();
            if !paths.contains(&directory) { paths.push(directory.clone()); }
            #[cfg(target_os = "ios")]
            { let bundled = directory.join("Frameworks"); if !paths.contains(&bundled) { paths.push(bundled); } }
            #[cfg(target_os = "ios")]
            if directory.extension().is_some_and(|extension| extension == "framework") {
                if let Some(frameworks) = directory.parent() {
                    let frameworks = frameworks.to_path_buf();
                    if !paths.contains(&frameworks) { paths.push(frameworks); }
                }
            }
        }
    }
    paths
}

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
fn repl_candidates() -> Vec<std::path::PathBuf> {
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

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) fn candidates(name: &str, policy: &super::NativeLoading) -> Vec<std::path::PathBuf> {
    if name == "z42_repl" { return repl_candidates(); }
    crate::pal::dynlib::candidate_paths(name, policy.search_dirs.clone())
}
