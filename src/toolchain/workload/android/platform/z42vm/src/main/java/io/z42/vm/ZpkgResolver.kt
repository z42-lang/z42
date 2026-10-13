package io.z42.vm

import android.content.res.AssetManager
import java.io.IOException

/**
 * Resolve a namespace (e.g. `"z42.core"`, `"Std.IO"`) to its zpkg
 * bytes. Returning `null` signals "this resolver doesn't know about
 * that namespace"; the runtime then falls back to other resolvers
 * (typically a search-path scan, which Android doesn't use).
 *
 * Spec: docs/design/runtime/embedding.md §11.
 */
interface ZpkgResolver {
    fun resolve(namespace: String): ByteArray?
}

/**
 * Default Android resolver. Builds a `namespace → bytes` map by
 * enumerating the `*.zpkg` files under `<subdir>` in the app's assets and reading each
 * one's `NSPC` section (via [Z42VM.readNamespaces]) — there is no
 * `index.json`. A single zpkg that ships several namespaces (e.g.
 * `z42.core.zpkg` provides `z42.core` + `Std` + `Std.Exceptions`) maps
 * all of them, which a `namespace == filename` guess could not.
 *
 * Spec: 变更 drop-index-json-self-describing/
 */
class AssetZpkgResolver(
    private val assets: AssetManager,
    private val subdir: String = "stdlib",
) : ZpkgResolver {
    private val byNamespace: Map<String, ByteArray> = loadNamespaceMap()

    override fun resolve(namespace: String): ByteArray? = byNamespace[namespace]

    /**
     * Enumerate the `*.zpkg` files under `<subdir>` and read each one's NSPC section to
     * assemble the namespace → bytes map. First-wins on duplicates;
     * filenames sorted for deterministic resolution.
     */
    private fun loadNamespaceMap(): Map<String, ByteArray> {
        val map = HashMap<String, ByteArray>()
        val files = try {
            assets.list(subdir)?.filter { it.endsWith(".zpkg") }?.sorted() ?: emptyList()
        } catch (_: IOException) {
            emptyList()
        }
        for (name in files) {
            val bytes = try {
                assets.open("$subdir/$name").use { it.readBytes() }
            } catch (_: IOException) {
                continue
            }
            for (ns in Z42VM.readNamespaces(bytes)) {
                if (!map.containsKey(ns)) map[ns] = bytes
            }
        }
        return map
    }
}

/**
 * HashMap-backed resolver useful for tests and for hosts that build
 * the zpkg dictionary at startup (e.g. fetched over network, or a REPL
 * that injects packages on demand).
 */
class MapZpkgResolver(
    initial: Map<String, ByteArray> = emptyMap(),
) : ZpkgResolver {
    private val map = HashMap(initial)

    fun set(namespace: String, bytes: ByteArray) {
        map[namespace] = bytes
    }

    override fun resolve(namespace: String): ByteArray? = map[namespace]
}
