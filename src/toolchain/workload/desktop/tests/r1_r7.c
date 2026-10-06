/* desktop R1–R7 — platform test harness for the Tier-1 C ABI (z42_host.h).
 *
 * The desktop peer of the wasm Playwright / iOS XCTest / Android JUnit R1–R7
 * suites: a real external C consumer links libz42.a and exercises the same
 * 7 embedding-contract scenarios. Driven by DesktopBackend
 * (scripts/xtask_test_desktop.z42) via `z42 xtask.zpkg test app desktop`.
 *
 * Usage:   r1_r7 <hello.zbc> <multi_line.zbc> <libs_dir>
 * Output:  one `[Rn] PASS` / `[Rn] FAIL: <msg>` line per scenario.
 * Exit:    0 iff all 7 pass, else 1 (2 = usage / setup error).
 *
 * Status-code contract (z42_host.h Z42HostStatus):
 *   R1 smoke         load+resolve+invoke hello → stdout "hello, world"
 *   R2 bad zbc       garbage bytes            → ERR_BAD_ZBC (10)
 *   R3 unknown entry resolve "Nope.Missing"   → ERR_ENTRY_NOT_FOUND (20)
 *   R4 arg mismatch  invoke 0-arg Main w/ 1   → ERR_ARG_MISMATCH (21)
 *   R5 resolver miss no search_paths          → load/invoke fails (stdlib unresolved)
 *   R6 lifecycle     init→…→shutdown ×3       → each cycle works
 *   R7 multi-line    invoke multi_line        → stdout "a\nb\nc" in order
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "z42_host.h"

/* ── stdout capture sink (no prefix; raw bytes) ─────────────────────────── */
static char   g_buf[65536];
static size_t g_len;
static void   sink(const char* bytes, size_t length, void* user_data) {
    (void)user_data;
    if (g_len + length < sizeof g_buf) { memcpy(g_buf + g_len, bytes, length); g_len += length; }
    g_buf[g_len] = '\0';
}
static void reset_capture(void) { g_len = 0; g_buf[0] = '\0'; }

/* ── tally ──────────────────────────────────────────────────────────────── */
static int g_pass = 0, g_fail = 0;
static void pass(const char* r)               { printf("[%s] PASS\n", r); g_pass++; }
static void fail(const char* r, const char* m){ printf("[%s] FAIL: %s\n", r, m); g_fail++; }

static unsigned char* read_file(const char* path, long* out_len) {
    FILE* f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    unsigned char* buf = (unsigned char*)malloc((size_t)n);
    if (!buf || fread(buf, 1, (size_t)n, f) != (size_t)n) { free(buf); fclose(f); return NULL; }
    fclose(f); *out_len = n; return buf;
}

static Z42HostConfig mkcfg(const char* const* paths) {
    Z42HostConfig c; memset(&c, 0, sizeof c);
    c.abi_version  = Z42_HOST_ABI_VERSION;
    c.struct_size  = sizeof c;
    c.exec_mode    = Z42_EXEC_MODE_INTERP;
    c.stdout_sink  = sink;
    c.search_paths = paths;
    return c;
}

int main(int argc, char** argv) {
    if (argc != 4) {
        fprintf(stderr, "usage: %s <hello.zbc> <multi_line.zbc> <libs_dir>\n", argv[0]);
        return 2;
    }
    const char* hello_path = argv[1];
    const char* multi_path = argv[2];
    const char* libs_dir   = argv[3];
    const char* paths[2] = { libs_dir, NULL };

    long hl = 0, ml = 0;
    unsigned char* hb = read_file(hello_path, &hl);
    unsigned char* mb = read_file(multi_path, &ml);
    if (!hb || !mb) { fprintf(stderr, "error: cannot read fixtures\n"); return 2; }

    /* R1 — smoke */
    {
        Z42HostConfig c = mkcfg(paths); Z42HostRef h = NULL;
        if (z42_host_initialize(&c, &h) != Z42_HOST_OK) { fail("R1", "initialize"); }
        else {
            Z42ModuleRef m = NULL; Z42EntryRef e = NULL; Z42Value r; reset_capture();
            if      (z42_host_load_zbc(h, hb, (size_t)hl, &m)      != Z42_HOST_OK) fail("R1", "load_zbc");
            else if (z42_host_resolve_entry(h, m, "Hello.Main", &e) != Z42_HOST_OK) fail("R1", "resolve_entry");
            else if (z42_host_invoke(e, NULL, 0, &r)               != Z42_HOST_OK) fail("R1", "invoke");
            else if (strstr(g_buf, "hello, world") == NULL)        fail("R1", "stdout mismatch");
            else pass("R1");
            z42_host_shutdown(h);
        }
    }

    /* R2 — bad zbc → BAD_ZBC (10) */
    {
        Z42HostConfig c = mkcfg(paths); Z42HostRef h = NULL; z42_host_initialize(&c, &h);
        unsigned char garbage[32]; for (int i = 0; i < 32; i++) garbage[i] = (unsigned char)(i + 1);
        Z42ModuleRef m = NULL;
        Z42HostStatus s = z42_host_load_zbc(h, garbage, sizeof garbage, &m);
        if (s == Z42_HOST_ERR_BAD_ZBC) pass("R2"); else fail("R2", "expected ERR_BAD_ZBC (10)");
        z42_host_shutdown(h);
    }

    /* R3 — unknown entry → ENTRY_NOT_FOUND (20) */
    {
        Z42HostConfig c = mkcfg(paths); Z42HostRef h = NULL; z42_host_initialize(&c, &h);
        Z42ModuleRef m = NULL; Z42EntryRef e = NULL;
        z42_host_load_zbc(h, hb, (size_t)hl, &m);
        Z42HostStatus s = z42_host_resolve_entry(h, m, "Nope.Missing", &e);
        if (s == Z42_HOST_ERR_ENTRY_NOT_FOUND) pass("R3"); else fail("R3", "expected ERR_ENTRY_NOT_FOUND (20)");
        z42_host_shutdown(h);
    }

    /* R4 — arg mismatch → ARG_MISMATCH (21): Hello.Main takes 0 args, pass 1 */
    {
        Z42HostConfig c = mkcfg(paths); Z42HostRef h = NULL; z42_host_initialize(&c, &h);
        Z42ModuleRef m = NULL; Z42EntryRef e = NULL; Z42Value r;
        z42_host_load_zbc(h, hb, (size_t)hl, &m);
        z42_host_resolve_entry(h, m, "Hello.Main", &e);
        Z42Value args[1]; memset(args, 0, sizeof args);
        Z42HostStatus s = z42_host_invoke(e, args, 1, &r);
        if (s == Z42_HOST_ERR_ARG_MISMATCH) pass("R4"); else fail("R4", "expected ERR_ARG_MISMATCH (21)");
        z42_host_shutdown(h);
    }

    /* R5 — resolver miss: no search_paths → stdlib unresolvable → load/invoke fails */
    {
        const char* no_paths[1] = { NULL };
        Z42HostConfig c = mkcfg(no_paths); Z42HostRef h = NULL; z42_host_initialize(&c, &h);
        Z42ModuleRef m = NULL; Z42EntryRef e = NULL; Z42Value r; reset_capture();
        Z42HostStatus ls = z42_host_load_zbc(h, hb, (size_t)hl, &m);
        Z42HostStatus is = Z42_HOST_OK;
        if (ls == Z42_HOST_OK && z42_host_resolve_entry(h, m, "Hello.Main", &e) == Z42_HOST_OK)
            is = z42_host_invoke(e, NULL, 0, &r);
        if (ls != Z42_HOST_OK || is != Z42_HOST_OK) pass("R5");
        else fail("R5", "expected failure with no search_paths");
        z42_host_shutdown(h);
    }

    /* R6 — lifecycle: init→load→invoke→shutdown ×3 */
    {
        int all_ok = 1;
        for (int i = 0; i < 3 && all_ok; i++) {
            Z42HostConfig c = mkcfg(paths); Z42HostRef h = NULL;
            if (z42_host_initialize(&c, &h) != Z42_HOST_OK) { all_ok = 0; break; }
            Z42ModuleRef m = NULL; Z42EntryRef e = NULL; Z42Value r; reset_capture();
            if (z42_host_load_zbc(h, hb, (size_t)hl, &m) == Z42_HOST_OK &&
                z42_host_resolve_entry(h, m, "Hello.Main", &e) == Z42_HOST_OK &&
                z42_host_invoke(e, NULL, 0, &r) == Z42_HOST_OK) {
                if (strstr(g_buf, "hello, world") == NULL) all_ok = 0;
            } else { all_ok = 0; }
            z42_host_shutdown(h);
        }
        if (all_ok) pass("R6"); else fail("R6", "repeat init/shutdown failed");
    }

    /* R7 — multi-line order: a\nb\nc */
    {
        Z42HostConfig c = mkcfg(paths); Z42HostRef h = NULL; z42_host_initialize(&c, &h);
        Z42ModuleRef m = NULL; Z42EntryRef e = NULL; Z42Value r; reset_capture();
        if      (z42_host_load_zbc(h, mb, (size_t)ml, &m)          != Z42_HOST_OK) fail("R7", "load_zbc");
        else if (z42_host_resolve_entry(h, m, "MultiLine.Main", &e) != Z42_HOST_OK) fail("R7", "resolve_entry");
        else if (z42_host_invoke(e, NULL, 0, &r)                   != Z42_HOST_OK) fail("R7", "invoke");
        else if (strstr(g_buf, "a\nb\nc") == NULL)                fail("R7", "multi-line order");
        else pass("R7");
        z42_host_shutdown(h);
    }

    free(hb); free(mb);
    printf("desktop R1-R7: %d passed, %d failed\n", g_pass, g_fail);
    return g_fail == 0 ? 0 : 1;
}
