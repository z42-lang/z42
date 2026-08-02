// Z42EmbeddedInstrumentedTest.kt — embedded test-host (add-wasm-testhost G6).
//
// Runs the bundled test corpus THROUGH the embedded VM, exactly as desktop /
// iOS / wasm do: Z42TestHost.runApp → z42_host_run_app → z42::app::run → the
// shared z42 test-agent → a JSON report file. Proves the SAME agent + bundle
// run on Android with a real filesystem (no VFS): the corpus is copied out of
// the test apk's assets to cacheDir, referenced by path, and the report read
// back.
//
// Assets (produced by `xtask test embedded --platform android`, exposed via the
// test Context.assets):
//   embedded/app/z42.testagent.zpkg
//   embedded/libs/*.zpkg
//   embedded/bundle/manifest.json + *.zbc
//
// Runs inside the emulator via `./gradlew :z42vm:connectedAndroidTest`.

package io.z42.vm

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class Z42EmbeddedInstrumentedTest {

    private val ctx by lazy {
        InstrumentationRegistry.getInstrumentation().context
    }

    /** Recursively copy an assets subtree to a real directory (z42 needs paths). */
    private fun copyAsset(path: String, dest: File) {
        val children = ctx.assets.list(path) ?: emptyArray()
        if (children.isEmpty()) {
            // Leaf file.
            dest.parentFile?.mkdirs()
            ctx.assets.open(path).use { input ->
                dest.outputStream().use { input.copyTo(it) }
            }
        } else {
            dest.mkdirs()
            for (child in children) {
                copyAsset("$path/$child", File(dest, child))
            }
        }
    }

    @Test
    fun embeddedBundle() {
        val root = File(ctx.cacheDir, "embedded")
        root.deleteRecursively()
        copyAsset("embedded", root)

        val app = File(root, "app/z42.testagent.zpkg").absolutePath
        val libs = File(root, "libs").absolutePath
        val manifest = File(root, "bundle/manifest.json").absolutePath
        val out = File(ctx.cacheDir, "z42-report.json").absolutePath

        val code = Z42TestHost.runApp(
            appPath = app, entry = null, libsDir = libs,
            args = arrayOf(manifest, "json", out),
        )
        assertEquals("embedded run exited non-zero", 0, code)

        val report = File(out).readText()
        assertTrue(
            "embedded report reports failures:\n$report",
            report.contains("\"failed\":0") || report.contains("\"failed\": 0"),
        )
    }
}
