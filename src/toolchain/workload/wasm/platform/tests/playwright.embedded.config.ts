import { defineConfig, devices } from '@playwright/test';

// Config for the EMBEDDED wasm test-host RUN (add-wasm-testhost G6), separate
// from playwright.config.ts (R1–R7 facade). Roots the web server at the
// self-contained deployable `artifacts/build/wasm-test/` (produced by
// `xtask test embedded --rid browser-wasm`) so index.html + run.js + files.json
// + app/libs/bundle resolve at the server root. Different port from the R1–R7
// harness so both can coexist.
const PORT = 4243;

// From this dir (src/toolchain/workload/wasm/platform/tests) up to the repo root
// is six levels; then the deployable.
const DEPLOY = '../../../../../../artifacts/build/wasm-test';

export default defineConfig({
    testDir: '.',
    testMatch: 'embedded.spec.ts',
    fullyParallel: false,
    workers: 1,
    // fix-wasm-embed-timeout: the whole-test budget. The in-browser corpus (each
    // case a fresh VmContext + stdlib reload on the single-threaded wasm interp) is
    // slow — the default 30s test timeout would abort before the in-page wait.
    // fix-wasm-shard-timeout: 620_000 (10.3min) → 1_500_000 (25min). The per-shard
    // corpus grew past the old 10min cap (shard 2 hit "Test timeout 620000ms
    // exceeded" deterministically), so give it 25min. The wasm CI job wall is 75min
    // and a cold "build deployable" takes ~27min, so 25min leaves comfortable slack.
    timeout: 1_500_000,
    use: {
        baseURL: `http://localhost:${PORT}`,
        actionTimeout: 10_000,
    },
    projects: [
        { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    ],
    webServer: {
        command: `npx http-server ${DEPLOY} -p ${PORT} -c-1 --cors -s`,
        port: PORT,
        reuseExistingServer: !process.env.CI,
        timeout: 30_000,
    },
    reporter: [['list']],
});
