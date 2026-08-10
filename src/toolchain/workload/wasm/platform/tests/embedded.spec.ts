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
    await page.goto('/');
    // run.js sets window.__done when the agent finishes (ok or error).
    await page.waitForFunction(() => (window as any).__done === true, { timeout: 60_000 });

    const err = await page.evaluate(() => (window as any).__error);
    expect(err, `embedded run errored: ${err}`).toBeFalsy();

    const report = await page.evaluate(() => (window as any).__report as string);
    expect(report, 'no report produced').toBeTruthy();

    const parsed = JSON.parse(report);
    expect(parsed.summary.total, 'empty corpus').toBeGreaterThan(0);
    expect(parsed.summary.failed, `failures in embedded report: ${report}`).toBe(0);
});
