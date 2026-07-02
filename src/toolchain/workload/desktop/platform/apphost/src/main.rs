//! Per-app native apphost stub (add-apphost, 2026-06-09).
//!
//! `z42 publish <project.z42.toml>` copies this binary and patches the embedded
//! target placeholder (located in the file by [`MAGIC`]) with the app's zpkg
//! path *relative to the produced exe*. At runtime the stub reads that path,
//! resolves a z42vm + libs (framework-dependent, local-first), and runs the
//! app's zpkg **directly**:
//!
//! ```text
//! z42vm <app.zpkg> -- <user argv>          (Z42_LIBS set)
//! ```
//!
//! No `launcher.zpkg`, no muxer, single VM — a deployed app needs only the
//! apphost exe + its zpkg + a resolvable runtime (z42vm+libs). The stub is the
//! only unavoidable native part: "find the VM + run the app". (The `z42` muxer
//! / launcher.zpkg still exist in the SDK for `z42 run/list/install/apphost
//! build`; the apphost just doesn't route through them.)

use std::env;
use std::path::PathBuf;
use std::process::exit;

use z42_hostrun::{ensure_portable_vm, exec_app, resolve_app_runtime};

/// 32-byte sentinel the patcher greps for in the on-disk binary. Followed by a
/// 992-byte payload holding the NUL-terminated app zpkg path (relative to the
/// exe dir); 1024 bytes total. `#[used]` + `#[no_mangle]` keep it addressable
/// and un-elided so the patcher can find it and the runtime can read it.
const MAGIC: [u8; 32] = *b"z42-apphost-target-v1-MAGIC-0001";
const PLACEHOLDER_LEN: usize = 1024;

#[used]
#[no_mangle]
pub static Z42_APPHOST_TARGET: [u8; PLACEHOLDER_LEN] = build_placeholder();

const fn build_placeholder() -> [u8; PLACEHOLDER_LEN] {
    let mut buf = [0u8; PLACEHOLDER_LEN];
    let mut i = 0;
    while i < MAGIC.len() {
        buf[i] = MAGIC[i];
        i += 1;
    }
    buf
}

/// Parse the embedded target from a placeholder buffer. `None` if unpatched
/// (payload still zeroed) or not valid UTF-8.
fn parse_target_bytes(buf: &[u8; PLACEHOLDER_LEN]) -> Option<PathBuf> {
    let payload = &buf[MAGIC.len()..];
    if payload[0] == 0 {
        return None; // unpatched template
    }
    let end = payload.iter().position(|&b| b == 0).unwrap_or(payload.len());
    let s = std::str::from_utf8(&payload[..end]).ok()?;
    if s.is_empty() {
        return None;
    }
    Some(PathBuf::from(s))
}

fn parse_target() -> Option<PathBuf> {
    // The patcher rewrites this static's bytes on disk *after* compilation, so
    // the optimizer must NOT assume its compile-time value (all-zero payload).
    // `#[used]` keeps the data in the binary, but without a volatile read a
    // release build (LTO/opt-z) const-folds `payload[0] == 0` to a constant
    // `true` and the patch has no effect (debug builds happen to work). A
    // volatile read of the whole array forces an actual memory load.
    let buf = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(Z42_APPHOST_TARGET)) };
    parse_target_bytes(&buf)
}

fn main() {
    let target = match parse_target() {
        Some(t) => t,
        None => {
            eprintln!("apphost: 未配置目标 app —— 占位符未被 `z42 publish` patch。");
            exit(1);
        }
    };

    let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = exe
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let app_zpkg = exe_dir.join(&target);

    // SDK-internal colocated bootstrap: if a z42vm ships in this package (next to
    // the apphost, or in `bin/`), pin it via $Z42_PORTABLE_VM so this app uses
    // its own ABI-matched vm AND any SDK app it spawns inherits the same one.
    ensure_portable_vm(&exe_dir);

    let rt = match resolve_app_runtime(&exe_dir) {
        Some(rt) => rt,
        None => {
            eprintln!(
                "apphost: 未找到 z42 运行时(z42vm)。已查 $Z42_HOME、{}（上行各级 .z42）、$HOME/.z42。\n\
                 请安装 z42 或设置 Z42_HOME。",
                exe_dir.display()
            );
            exit(1);
        }
    };

    // Run the app's zpkg directly: z42vm <app.zpkg> -- <user argv> (Z42_LIBS set).
    let user_args: Vec<String> = env::args().skip(1).collect();
    exec_app(&rt, &app_zpkg, &user_args);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patched(path: &str) -> [u8; PLACEHOLDER_LEN] {
        let mut buf = build_placeholder();
        let bytes = path.as_bytes();
        buf[MAGIC.len()..MAGIC.len() + bytes.len()].copy_from_slice(bytes);
        // NUL terminator already present (tail is zeroed).
        buf
    }

    #[test]
    fn unpatched_is_none() {
        assert_eq!(parse_target_bytes(&build_placeholder()), None);
    }

    #[test]
    fn patched_roundtrips() {
        let buf = patched("myapp.zpkg");
        assert_eq!(parse_target_bytes(&buf), Some(PathBuf::from("myapp.zpkg")));
    }

    #[test]
    fn magic_preserved_after_patch() {
        let buf = patched("a.zpkg");
        assert_eq!(&buf[..MAGIC.len()], &MAGIC[..]);
    }
}
