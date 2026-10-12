// playwright tests for @z42/wasm facade — implements platform-test-contract
// R1–R7. Page setup + actual VM driving lives in host.js; this file just
// invokes window.__test.runRn and asserts on the returned value.
//
// Spec: 变更 add-wasm-tests/

import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
    await page.goto('/tests/');
    await page.waitForFunction(() => (window as unknown as { __ready?: boolean }).__ready === true);
});

test('R1 smoke / hello world', async ({ page }) => {
    const out = await page.evaluate(() => (window as any).__test.runR1());
    expect(out).toBe('hello, world\n');
});

test('R2 error / bad zbc throws status 10', async ({ page }) => {
    const result = await page.evaluate(() => (window as any).__test.runR2());
    expect(result.thrown).toBe(true);
    expect(result.status).toBe(10);
});

test('R3 error / unknown entry throws status 20', async ({ page }) => {
    const result = await page.evaluate(() => (window as any).__test.runR3());
    expect(result.thrown).toBe(true);
    expect(result.status).toBe(20);
    expect(result.messageContainsFqn).toBe(true);
});

test('R4 error / wrong arg count throws status 21', async ({ page }) => {
    const result = await page.evaluate(() => (window as any).__test.runR4());
    expect(result.thrown).toBe(true);
    expect(result.status).toBe(21);
});

test('R5 resolver miss surfaces at load/invoke', async ({ page }) => {
    const result = await page.evaluate(() => (window as any).__test.runR5());
    expect(result.thrown).toBe(true);
    // Acceptable: BadZbc (10) or VmException (30) — both signal stdlib miss.
    expect([10, 30]).toContain(result.status);
});

test('R6 lifecycle / repeat init', async ({ page }) => {
    const outputs = await page.evaluate(() => (window as any).__test.runR6());
    expect(outputs).toEqual(['hello, world\n', 'hello, world\n', 'hello, world\n']);
});

test('R7 stdout / multi-line preserves order', async ({ page }) => {
    const out = await page.evaluate(() => (window as any).__test.runR7());
    expect(out).toBe('a\nb\nc\n');
});
