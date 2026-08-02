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
