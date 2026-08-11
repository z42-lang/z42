// Playwright RUN for the embedded wasm test-host (add-wasm-testhost G6).
//
// Distinct from r1-r7.spec.ts (which drives the handle-based facade for the
// embedding-API contract): this serves the self-contained deployable produced
// by `xtask test embedded --rid browser-wasm` (artifacts/build/wasm-test/),
// opens its index.html — which runs the shared z42 test-agent over the bundled
// corpus entirely in-browser (runTestApp → z42::app::run) — and asserts the
// JSON report has no failures. Run via playwright.embedded.config.ts, whose
// webServer roots at the deployable dir.
import { test, expect } from '@playwright/test';

test('embedded test-host: bundled corpus runs green in-browser', async ({ page }) => {
    // diagnose-mobile-wasm-embed: capture the browser console + page errors so a
    // wasm panic (surfaced by the panic hook via console.error) shows up in the
    // CI log — otherwise the failure is just the opaque "RuntimeError: unreachable".
    const consoleLogs: string[] = [];
    page.on('console', (m) => consoleLogs.push(`[${m.type()}] ${m.text()}`));
    page.on('pageerror', (e) => consoleLogs.push(`[pageerror] ${e.message}`));

    await page.goto('/');
    // run.js sets window.__done when the agent finishes (ok or error).
    // fix-wasm-embed-timeout: `{timeout}` must be the 3rd arg (options), NOT the
    // 2nd (`arg`) — passed as arg it was silently ignored, so this fell back to
    // `actionTimeout` (10s) and timed out before the (slow, single-threaded wasm
    // interp) corpus of ~283 goldens could finish. Give it a generous budget.
    await page.waitForFunction(() => (window as any).__done === true, undefined, { timeout: 600_000 });

    // Always echo captured console into the test output (visible in CI).
    if (consoleLogs.length) console.log('--- browser console ---\n' + consoleLogs.join('\n'));

    const err = await page.evaluate(() => (window as any).__error);
    expect(err, `embedded run errored: ${err}\nconsole:\n${consoleLogs.join('\n')}`).toBeFalsy();

    const report = await page.evaluate(() => (window as any).__report as string);
    expect(report, 'no report produced').toBeTruthy();

    const parsed = JSON.parse(report);
    expect(parsed.summary.total, 'empty corpus').toBeGreaterThan(0);
    expect(parsed.summary.failed, `failures in embedded report: ${report}`).toBe(0);
});
