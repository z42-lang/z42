/* Generic self-contained z42 app host (embed model) — the SELF-CONTAINED
 * counterpart to the spawn apphost (workload/desktop/platform/apphost, which
 * execs an external z42vm). This one links libz42 and runs the app IN-PROCESS
 * via z42_host_run_app (the G6 embed core), so the published app carries no
 * dependency on an installed z42vm.
 *
 * Generic + prebuilt: it resolves its OWN executable directory and runs the
 * co-located `app.zpkg` against `./libs` — so the desktop workload ships ONE
 * prebuilt apphost (static: libz42 baked in; dynamic: + libz42.{dylib,so,dll}
 * beside the exe), and `z42 publish --rid <desktop> --self-contained` just
 * copies it next to the app's zpkg + staged libs. No compile at publish time.
 *
 * Layout the published app expects (all co-located with the exe):
 *   <app>/            <exe>  +  app.zpkg  +  libs/*.zpkg  [+ libz42.<dyn> if dynamic]
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>

#if defined(__APPLE__)
#  include <mach-o/dyld.h>
#  include <libgen.h>
#elif defined(_WIN32)
#  include <windows.h>
#else
#  include <libgen.h>
#  include <unistd.h>
#endif

extern int z42_host_run_app(const char *app_zpkg, const char *entry,
                            const char *libs_dir, int argc,
                            const char *const *argv);

/* Fill `dir` with the directory containing this executable. Returns 0 on ok. */
static int exe_dir(char *dir, size_t n) {
#if defined(__APPLE__)
    char exe[PATH_MAX];
    uint32_t sz = sizeof exe;
    if (_NSGetExecutablePath(exe, &sz) != 0) return -1;
    char real[PATH_MAX];
    const char *d = realpath(exe, real) ? dirname(real) : dirname(exe);
    snprintf(dir, n, "%s", d);
    return 0;
#elif defined(_WIN32)
    char exe[MAX_PATH];
    DWORD len = GetModuleFileNameA(NULL, exe, MAX_PATH);
    if (len == 0 || len == MAX_PATH) return -1;
    for (DWORD i = len; i > 0; --i) {
        if (exe[i - 1] == '\\' || exe[i - 1] == '/') { exe[i - 1] = '\0'; break; }
    }
    snprintf(dir, n, "%s", exe);
    return 0;
#else
    char exe[PATH_MAX];
    ssize_t len = readlink("/proc/self/exe", exe, sizeof exe - 1);
    if (len < 0) return -1;
    exe[len] = '\0';
    snprintf(dir, n, "%s", dirname(exe));
    return 0;
#endif
}

int main(int argc, char **argv) {
    char dir[PATH_MAX];
    if (exe_dir(dir, sizeof dir) != 0) {
        fprintf(stderr, "z42 apphost: cannot resolve executable directory\n");
        return 71;
    }
    char app[PATH_MAX], libs[PATH_MAX];
#if defined(_WIN32)
    snprintf(app,  sizeof app,  "%s\\app.zpkg", dir);
    snprintf(libs, sizeof libs, "%s\\libs", dir);
#else
    snprintf(app,  sizeof app,  "%s/app.zpkg", dir);
    snprintf(libs, sizeof libs, "%s/libs", dir);
#endif
    /* Forward argv[1..] to the app's GetCommandLineArgs(). */
    return z42_host_run_app(app, NULL, libs, argc - 1,
                            (const char *const *)(argv + 1));
}
