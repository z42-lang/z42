package io.z42.vm

import android.content.res.AssetManager
import java.io.FileNotFoundException
import java.util.concurrent.ConcurrentHashMap

/** Supply bytes for a named Z42 program, dependency, or runtimeconfig. */
fun interface FileResolver {
    fun resolve(fileName: String, kind: Int): ByteArray?

    companion object {
        const val ZPKG = 1
        const val ZBC = 2
        const val RUNTIME_CONFIG = 3
        const val ZBC_DEPS = 4
    }
}

class AssetZpkgResolver(
    private val assets: AssetManager,
    private val subdir: String = "stdlib",
) : FileResolver {
    override fun resolve(fileName: String, kind: Int): ByteArray? {
        val fromApp = kind == FileResolver.RUNTIME_CONFIG || kind == FileResolver.ZBC_DEPS
        val path = if (fromApp) fileName else "$subdir/$fileName"
        return try {
            assets.open(path).use { it.readBytes() }
        } catch (_: FileNotFoundException) {
            null
        }
    }
}

class MapZpkgResolver(initial: Map<String, ByteArray> = emptyMap()) : FileResolver {
    private val files = ConcurrentHashMap(initial)

    fun set(fileName: String, bytes: ByteArray) {
        files[fileName] = bytes
    }

    override fun resolve(fileName: String, kind: Int): ByteArray? = files[fileName]
}
