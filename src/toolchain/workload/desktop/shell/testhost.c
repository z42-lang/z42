/* Desktop self-contained test-host shell (add-embedded-app-run, 2026-08-02).
 *
 * A minimal native shell that EMBEDS the VM (links libz42) and runs a z42
 * app.zpkg via the one-shot C ABI `z42_host_run_app` — the same embedding
 * entry Swift/JNI/wasm shells bind. Together with the shared z42 test-agent,
 * this is the desktop reference for "build app -> embed VM -> run tests":
 * desktop uses self-contained embedding just like mobile (unified path).
 *
 * Link (static): cc testhost.c -L<libdir> -lz42 <native libs> -o testhost
 * Link (dynamic): same, but resolving libz42.dylib/.so instead of libz42.a.
 *
 * Usage:  Z42_LIBS=<libs> ./testhost <app.zpkg> [prog args... e.g. target.zbc format]
 */
#include <stdlib.h>

/* Exported by libz42 (src/runtime/src/host/mod.rs). entry/libs_dir may be NULL;
 * argv[0..argc] forward to the app's GetCommandLineArgs(). */
extern int z42_host_run_app(const char *app_zpkg,
                            const char *entry,
                            const char *libs_dir,
                            int argc,
                            const char *const *argv);

int main(int argc, char **argv) {
    if (argc < 2) {
        return 2; /* usage: testhost <app.zpkg> [prog args...] */
    }
    const char *app = argv[1];
    const char *libs = getenv("Z42_LIBS"); /* may be NULL */
    /* Forward argv[2..] (e.g. <target.zbc> <format>) to the embedded app. */
    return z42_host_run_app(app, NULL, libs, argc - 2, (const char *const *)(argv + 2));
}
