// Z42VMInstrumentedTest.kt — JUnit instrumented test implementation of
// platform-test-contract R1–R7. Runs inside the Android emulator
// (AVD z42_pixel6_api37) via `./test.sh` → `./gradlew :z42vm:
// connectedAndroidTest`.
//
// Resources (produced by `../build.sh`, exposed via `Context.assets`):
//   test-fixtures/hello.zbc        — single line "hello, world"
//   test-fixtures/multi_line.zbc   — three lines "a" / "b" / "c"
//   stdlib/*.zpkg                  — corelib + Std.IO + ... (NSPC self-describing)
//
// Spec: docs/spec/archive/2026-05-12-add-android-tests/specs/android-tests/spec.md
//       docs/spec/archive/2026-05-12-define-platform-test-contract/specs/platform-test-contract/spec.md

package io.z42.vm

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class Z42VMInstrumentedTest {

    // ── Helpers ─────────────────────────────────────────────────────────

    private val ctx by lazy {
        // testContext (the Z42VMTests apk) holds androidTest/assets/*.
        InstrumentationRegistry.getInstrumentation().context
    }

    /** Read `test-fixtures/<name>.zbc` from the test bundle assets. */
    private fun fixture(name: String): ByteArray =
        ctx.assets.open("test-fixtures/$name.zbc").use { it.readBytes() }

    /** Build a VM with the default AssetZpkgResolver + a collecting sink. */
    private fun makeVMWithSink(): Pair<Z42VM, Collector> {
        val coll = Collector()
        val vm = Z42VM(
            zpkgResolver = AssetZpkgResolver(ctx.assets, "stdlib"),
            stdoutHandler = { bytes -> coll.append(bytes) },
        )
        return vm to coll
    }

    /** Run hello.zbc to completion; return collected stdout as UTF-8. */
    private fun runHelloAndCollect(): String {
        val (vm, coll) = makeVMWithSink()
        vm.use {
            val mod = vm.loadZbc(fixture("hello"))
            val entry = vm.resolveEntry(mod, "Hello.Main")
            vm.invoke(entry)
        }
        return coll.text
    }

    // ── R1 ─ Smoke ──────────────────────────────────────────────────────

    @Test
    fun testSmokeHelloWorld() {
        assertEquals("hello, world\n", runHelloAndCollect())
    }

    // ── R2 ─ Bad zbc → status 10 ────────────────────────────────────────

    @Test
    fun testBadZbcThrowsBadZbc() {
        makeVMWithSink().first.use { vm ->
            val garbage = byteArrayOf(
                0xDE.toByte(), 0xAD.toByte(), 0xBE.toByte(), 0xEF.toByte(),
                0x00, 0x01, 0x02, 0x03,
            )
            try {
                vm.loadZbc(garbage)
                fail("expected Z42VMException for garbage zbc")
            } catch (e: Z42VMException) {
                assertEquals("expected badZbc (status 10)", 10, e.status)
            }
        }
    }

    // ── R3 ─ Unknown entry → status 20 ──────────────────────────────────

    @Test
    fun testResolveUnknownEntryThrowsEntryNotFound() {
        makeVMWithSink().first.use { vm ->
            val mod = vm.loadZbc(fixture("hello"))
            try {
                vm.resolveEntry(mod, "App.Ghost")
                fail("expected Z42VMException for unknown FQN")
            } catch (e: Z42VMException) {
                assertEquals("expected entryNotFound (status 20)", 20, e.status)
                assertTrue(
                    "error message should mention unknown FQN, got: ${e.message}",
                    (e.message ?: "").contains("App.Ghost"),
                )
            }
        }
    }

    // ── R4 ─ Wrong arg count → status 21 ────────────────────────────────

    @Test
    fun testInvokeWrongArgCountThrowsArgMismatch() {
        makeVMWithSink().first.use { vm ->
            val mod = vm.loadZbc(fixture("hello"))
            val entry = vm.resolveEntry(mod, "Hello.Main")
            // Hello.Main is `void Main()`; one extra arg trips the arg-count check.
            try {
                vm.invoke(entry, Z42VMValue.I64(42))
                fail("expected Z42VMException for wrong arg count")
            } catch (e: Z42VMException) {
                assertEquals("expected argMismatch (status 21)", 21, e.status)
            }
        }
    }

    // ── R5 ─ MapResolver missing corelib → status 10/30 ─────────────────

    @Test
    fun testMapResolverWithoutCorelibSurfacesAtInvoke() {
        val resolver = MapZpkgResolver(mapOf("Std.Phantom" to ByteArray(0)))
        val vm = Z42VM(zpkgResolver = resolver, stdoutHandler = { /* discard */ })
        vm.use {
            try {
                val mod = vm.loadZbc(fixture("hello"))
                val entry = vm.resolveEntry(mod, "Hello.Main")
                vm.invoke(entry)
                fail("expected hello.zbc to fail under empty resolver")
            } catch (e: Z42VMException) {
                assertTrue(
                    "expected badZbc (10) or vmException (30); got ${e.status} — ${e.message}",
                    e.status == 10 || e.status == 30,
                )
            }
        }
    }

    // ── R6 ─ Init / shutdown × 3 rounds ─────────────────────────────────

    @Test
    fun testInitShutdownLifecycleRoundtrip() {
        for (i in 1..3) {
            assertEquals("iteration $i mismatch", "hello, world\n", runHelloAndCollect())
        }
    }

    // ── R7 ─ Multi-line stdout preserves byte order ─────────────────────

    @Test
    fun testMultiLineStdoutPreservesOrder() {
        val (vm, coll) = makeVMWithSink()
        vm.use {
            val mod = vm.loadZbc(fixture("multi_line"))
            val entry = vm.resolveEntry(mod, "MultiLine.Main")
            vm.invoke(entry)
        }
        // Contract D3: assert accumulated bytes, not per-callback shape.
        assertEquals("a\nb\nc\n", coll.text)
    }
}

/** Thread-safe byte accumulator for sink callbacks. */
private class Collector {
    private val buf = ByteArrayOutputStream()
    private val lock = Any()

    fun append(bytes: ByteArray) {
        synchronized(lock) { buf.write(bytes) }
    }

    val text: String
        get() = synchronized(lock) { buf.toByteArray() }.toString(Charsets.UTF_8)
}
