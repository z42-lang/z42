/* JNI bridge — `io.z42.vm.Z42VM` Kotlin facade → z42_host_* C ABI.
 *
 * Spec: docs/spec/archive/2026-05-12-add-platform-android/
 *       docs/design/runtime/embedding.md §6.2 (Tier 3 Android)
 *
 * Threading model: v0.1 the runtime is a process singleton; Kotlin
 * callers serialise their own access. JNI callbacks happen on the
 * same thread that called into z42 (stdoutHandler / zpkgResolver
 * trampolines re-enter via AttachCurrentThread if the runtime were
 * to ever spawn — v0.1 does not).
 *
 * Lifetime: the Kotlin Z42VM instance owns the resolver / sink boxes
 * (Java global refs taken in nativeInitialize, released in
 * nativeShutdown). The runtime holds the C function pointers via
 * Z42HostConfig until z42_host_shutdown.
 */

#include <jni.h>
#include <stdlib.h>
#include <string.h>

#include "z42_host.h"

/* ── Cached JavaVM + Kotlin class / method IDs ────────────────────────── */

static JavaVM* g_jvm = NULL;

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM* vm, void* reserved) {
    (void)reserved;
    g_jvm = vm;
    return JNI_VERSION_1_6;
}

/* Per-instance context kept alive for the VM's lifetime. Lives in the
 * runtime's user_data slot; freed in nativeShutdown. */
typedef struct {
    jobject resolver_global;       /* io.z42.vm.ZpkgResolver instance */
    jobject stdout_handler_global; /* kotlin.jvm.functions.Function1<ByteArray, Unit>?, may be NULL */
    jobject stderr_handler_global; /* same */
    jmethodID resolver_resolve;    /* (Ljava/lang/String;)[B */
    jmethodID function1_invoke;    /* (Ljava/lang/Object;)Ljava/lang/Object; */
    /* Temporary byte buffer the resolver trampoline returns; freed at
     * the start of the next resolve call or in nativeShutdown. */
    jbyteArray last_zpkg_global;
    void* last_zpkg_bytes;
    jsize last_zpkg_len;
} JniContext;

/* Packed handle returned to Kotlin as `nativeHandle: Long`. Carries the
 * runtime VM pointer + this binding's JniContext so nativeLoadZbc /
 * Invoke / Shutdown can recover both with O(1) cost. */
typedef struct {
    Z42HostRef host;
    JniContext* ctx;
} Z42Handle;

/* Helper: throw io.z42.vm.Z42VMException via JNI. */
static void throw_z42_vm_exception(JNIEnv* env, int status, const char* msg) {
    jclass cls = (*env)->FindClass(env, "io/z42/vm/Z42VMException");
    if (cls == NULL) return;
    jmethodID ctor = (*env)->GetMethodID(env, cls, "<init>", "(ILjava/lang/String;)V");
    if (ctor == NULL) return;
    jstring jmsg = (*env)->NewStringUTF(env, msg ? msg : "");
    jobject exc = (*env)->NewObject(env, cls, ctor, (jint)status, jmsg);
    (*env)->Throw(env, (jthrowable)exc);
}

static void throw_last_error(JNIEnv* env) {
    Z42Error err = z42_host_last_error(NULL);
    const char* msg = err.message ? err.message : "";
    throw_z42_vm_exception(env, (int)err.code, msg);
}

/* ── zpkg resolver trampoline ─────────────────────────────────────────── */

static int zpkg_resolver_trampoline(
    const char* namespace_name,
    const uint8_t** out_bytes,
    size_t* out_length,
    void* user_data
) {
    JniContext* ctx = (JniContext*)user_data;
    if (!g_jvm || !ctx || !ctx->resolver_global) return 0;

    JNIEnv* env = NULL;
    if ((*g_jvm)->GetEnv(g_jvm, (void**)&env, JNI_VERSION_1_6) != JNI_OK) return 0;

    jstring jns = (*env)->NewStringUTF(env, namespace_name);
    jobject result = (*env)->CallObjectMethod(
        env, ctx->resolver_global, ctx->resolver_resolve, jns);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        (*env)->DeleteLocalRef(env, jns);
        return 0;
    }
    (*env)->DeleteLocalRef(env, jns);
    if (result == NULL) return 0;

    /* Release any prior buffer first. */
    if (ctx->last_zpkg_global) {
        if (ctx->last_zpkg_bytes) {
            (*env)->ReleaseByteArrayElements(
                env, ctx->last_zpkg_global, (jbyte*)ctx->last_zpkg_bytes, JNI_ABORT);
        }
        (*env)->DeleteGlobalRef(env, ctx->last_zpkg_global);
        ctx->last_zpkg_global = NULL;
        ctx->last_zpkg_bytes = NULL;
        ctx->last_zpkg_len = 0;
    }

    jbyteArray arr = (jbyteArray)result;
    ctx->last_zpkg_global = (jbyteArray)(*env)->NewGlobalRef(env, arr);
    (*env)->DeleteLocalRef(env, arr);

    ctx->last_zpkg_len = (*env)->GetArrayLength(env, ctx->last_zpkg_global);
    ctx->last_zpkg_bytes = (*env)->GetByteArrayElements(
        env, ctx->last_zpkg_global, NULL);

    *out_bytes = (const uint8_t*)ctx->last_zpkg_bytes;
    *out_length = (size_t)ctx->last_zpkg_len;
    return 1;
}

/* ── stdout / stderr sink trampolines ──────────────────────────────────── */

static void invoke_sink(jobject handler_global, const char* bytes, size_t length) {
    if (!g_jvm || !handler_global) return;
    JNIEnv* env = NULL;
    if ((*g_jvm)->GetEnv(g_jvm, (void**)&env, JNI_VERSION_1_6) != JNI_OK) return;

    jbyteArray arr = (*env)->NewByteArray(env, (jsize)length);
    (*env)->SetByteArrayRegion(env, arr, 0, (jsize)length, (const jbyte*)bytes);

    jclass cls = (*env)->GetObjectClass(env, handler_global);
    jmethodID invoke = (*env)->GetMethodID(
        env, cls, "invoke", "(Ljava/lang/Object;)Ljava/lang/Object;");
    jobject result = (*env)->CallObjectMethod(env, handler_global, invoke, arr);
    if ((*env)->ExceptionCheck(env)) (*env)->ExceptionClear(env);
    if (result) (*env)->DeleteLocalRef(env, result);
    (*env)->DeleteLocalRef(env, arr);
    (*env)->DeleteLocalRef(env, cls);
}

static void stdout_sink_trampoline(const char* bytes, size_t length, void* user_data) {
    JniContext* ctx = (JniContext*)user_data;
    if (ctx) invoke_sink(ctx->stdout_handler_global, bytes, length);
}

static void stderr_sink_trampoline(const char* bytes, size_t length, void* user_data) {
    JniContext* ctx = (JniContext*)user_data;
    if (ctx) invoke_sink(ctx->stderr_handler_global, bytes, length);
}

/* ── JNI exports ──────────────────────────────────────────────────────── */

JNIEXPORT jlong JNICALL
Java_io_z42_vm_Z42VM_nativeInitialize(
    JNIEnv* env, jobject self,
    jobject resolver,
    jobject stdoutHandler,
    jobject stderrHandler
) {
    (void)self;
    JniContext* ctx = (JniContext*)calloc(1, sizeof(JniContext));
    if (!ctx) {
        throw_z42_vm_exception(env, 99, "calloc(JniContext) failed");
        return 0;
    }
    ctx->resolver_global = (*env)->NewGlobalRef(env, resolver);
    jclass resolver_cls = (*env)->GetObjectClass(env, resolver);
    ctx->resolver_resolve = (*env)->GetMethodID(
        env, resolver_cls, "resolve", "(Ljava/lang/String;)[B");
    (*env)->DeleteLocalRef(env, resolver_cls);

    if (stdoutHandler != NULL) {
        ctx->stdout_handler_global = (*env)->NewGlobalRef(env, stdoutHandler);
    }
    if (stderrHandler != NULL) {
        ctx->stderr_handler_global = (*env)->NewGlobalRef(env, stderrHandler);
    }

    Z42HostConfig cfg = {0};
    cfg.abi_version = Z42_HOST_ABI_VERSION;
    cfg.exec_mode = Z42_EXEC_MODE_INTERP;
    cfg.zpkg_resolver = zpkg_resolver_trampoline;
    cfg.zpkg_resolver_user_data = ctx;
    if (ctx->stdout_handler_global) {
        cfg.stdout_sink = stdout_sink_trampoline;
        cfg.sink_user_data = ctx;
    }

    Z42HostRef host = NULL;
    if (z42_host_initialize(&cfg, &host) != Z42_HOST_OK) {
        throw_last_error(env);
        if (ctx->resolver_global) (*env)->DeleteGlobalRef(env, ctx->resolver_global);
        if (ctx->stdout_handler_global) (*env)->DeleteGlobalRef(env, ctx->stdout_handler_global);
        if (ctx->stderr_handler_global) (*env)->DeleteGlobalRef(env, ctx->stderr_handler_global);
        free(ctx);
        return 0;
    }

    /* stderr_sink installed separately so each sink has its own user_data;
     * Tier 1 supports independent stdout / stderr via dedicated setters. */
    if (ctx->stderr_handler_global) {
        z42_host_set_stderr_sink(host, stderr_sink_trampoline, ctx);
    }

    /* The Kotlin side stores `nativeHandle: Long`. Pack (host, ctx) into
     * a small malloc'd Z42Handle so we can recover both via host_of /
     * ctx_of helpers. */
    Z42Handle* handle = (Z42Handle*)malloc(sizeof(Z42Handle));
    handle->host = host;
    handle->ctx = ctx;
    return (jlong)(uintptr_t)handle;
}

static Z42HostRef host_of(jlong h) { return ((Z42Handle*)(uintptr_t)h)->host; }
static JniContext* ctx_of(jlong h) { return ((Z42Handle*)(uintptr_t)h)->ctx; }

JNIEXPORT jlong JNICALL
Java_io_z42_vm_Z42VM_nativeLoadZbc(
    JNIEnv* env, jobject self,
    jlong handle,
    jbyteArray bytes
) {
    (void)self;
    Z42HostRef host = host_of(handle);
    jsize len = (*env)->GetArrayLength(env, bytes);
    jbyte* buf = (*env)->GetByteArrayElements(env, bytes, NULL);

    Z42ModuleRef mod = NULL;
    Z42HostStatus status = z42_host_load_zbc(host, (const uint8_t*)buf, (size_t)len, &mod);
    (*env)->ReleaseByteArrayElements(env, bytes, buf, JNI_ABORT);

    if (status != Z42_HOST_OK) {
        throw_last_error(env);
        return 0;
    }
    return (jlong)(uintptr_t)mod;
}

JNIEXPORT jlong JNICALL
Java_io_z42_vm_Z42VM_nativeResolveEntry(
    JNIEnv* env, jobject self,
    jlong handle,
    jlong module,
    jstring fqn
) {
    (void)self;
    Z42HostRef host = host_of(handle);
    Z42ModuleRef mod = (Z42ModuleRef)(uintptr_t)module;

    const char* fqn_cstr = (*env)->GetStringUTFChars(env, fqn, NULL);
    Z42EntryRef entry = NULL;
    Z42HostStatus status = z42_host_resolve_entry(host, mod, fqn_cstr, &entry);
    (*env)->ReleaseStringUTFChars(env, fqn, fqn_cstr);

    if (status != Z42_HOST_OK) {
        throw_last_error(env);
        return 0;
    }
    return (jlong)(uintptr_t)entry;
}

JNIEXPORT jlongArray JNICALL
Java_io_z42_vm_Z42VM_nativeInvoke(
    JNIEnv* env, jobject self,
    jlong entry,
    jintArray argTags,
    jlongArray argPayloads
) {
    (void)self;
    Z42EntryRef e = (Z42EntryRef)(uintptr_t)entry;
    jsize n = (*env)->GetArrayLength(env, argTags);

    Z42Value* args = NULL;
    if (n > 0) {
        args = (Z42Value*)calloc((size_t)n, sizeof(Z42Value));
        jint* tags = (*env)->GetIntArrayElements(env, argTags, NULL);
        jlong* payloads = (*env)->GetLongArrayElements(env, argPayloads, NULL);
        for (jsize i = 0; i < n; ++i) {
            args[i].tag = (uint32_t)tags[i];
            args[i].reserved = 0;
            args[i].payload = (uint64_t)payloads[i];
        }
        (*env)->ReleaseIntArrayElements(env, argTags, tags, JNI_ABORT);
        (*env)->ReleaseLongArrayElements(env, argPayloads, payloads, JNI_ABORT);
    }

    Z42Value result;
    memset(&result, 0, sizeof(result));
    Z42HostStatus status = z42_host_invoke(e, args, (size_t)n, &result);
    free(args);

    if (status != Z42_HOST_OK) {
        throw_last_error(env);
        return NULL;
    }

    jlongArray out = (*env)->NewLongArray(env, 2);
    jlong tagAndPayload[2] = { (jlong)result.tag, (jlong)result.payload };
    (*env)->SetLongArrayRegion(env, out, 0, 2, tagAndPayload);
    return out;
}

JNIEXPORT void JNICALL
Java_io_z42_vm_Z42VM_nativeShutdown(JNIEnv* env, jobject self, jlong handle) {
    (void)self;
    Z42Handle* h = (Z42Handle*)(uintptr_t)handle;
    if (!h) return;
    z42_host_shutdown(h->host);

    JniContext* ctx = h->ctx;
    if (ctx) {
        if (ctx->last_zpkg_global) {
            if (ctx->last_zpkg_bytes) {
                (*env)->ReleaseByteArrayElements(
                    env, ctx->last_zpkg_global,
                    (jbyte*)ctx->last_zpkg_bytes, JNI_ABORT);
            }
            (*env)->DeleteGlobalRef(env, ctx->last_zpkg_global);
        }
        if (ctx->resolver_global) (*env)->DeleteGlobalRef(env, ctx->resolver_global);
        if (ctx->stdout_handler_global) (*env)->DeleteGlobalRef(env, ctx->stdout_handler_global);
        if (ctx->stderr_handler_global) (*env)->DeleteGlobalRef(env, ctx->stderr_handler_global);
        free(ctx);
    }
    free(h);
}

/* ── one-shot embedded app-run bridge (add-wasm-testhost G6) ──────────────
 *
 * `io.z42.vm.Z42TestHost.runApp` → z42_host_run_app: load a bundled app.zpkg
 * and run its entry (no Z42VM handle — it builds + tears down its own VM),
 * sourcing the stdlib from `libsDir`. Android has a real filesystem, so the
 * agent + stdlib + bundle are copied out of assets to cacheDir and referenced
 * by path (no VFS). The agent writes its JSON report to the out-path arg; the
 * caller reads it back. Same core (z42::app::run) as desktop / iOS.
 */
JNIEXPORT jint JNICALL
Java_io_z42_vm_Z42TestHost_nativeRunApp(
    JNIEnv* env, jclass clazz,
    jstring appPath, jstring entry, jstring libsDir, jobjectArray args
) {
    (void)clazz;
    const char* app  = (*env)->GetStringUTFChars(env, appPath, NULL);
    const char* ent  = entry   ? (*env)->GetStringUTFChars(env, entry, NULL)   : NULL;
    const char* libs = libsDir ? (*env)->GetStringUTFChars(env, libsDir, NULL) : NULL;

    jsize argc = args ? (*env)->GetArrayLength(env, args) : 0;
    size_t slots = (size_t)(argc > 0 ? argc : 1);
    const char** argv = (const char**)calloc(slots, sizeof(char*));
    jstring* jargs = (jstring*)calloc(slots, sizeof(jstring));
    for (jsize i = 0; i < argc; i++) {
        jargs[i] = (jstring)(*env)->GetObjectArrayElement(env, args, i);
        argv[i] = (*env)->GetStringUTFChars(env, jargs[i], NULL);
    }

    // fix-android-embed-oom: the embedded test-host runs the whole corpus, each
    // golden in a fresh VmContext (heap + full stdlib). The agent defaults to
    // parallel workers (= emulator cores), so peak memory = outer agent + N
    // golden contexts → the low-memory-killer killed the app (~1.2GB RSS on the
    // emulator). Force sequential here (peak = outer + 1 context) unless the
    // caller set it explicitly (overwrite=0). Same-process setenv → the agent's
    // getenv sees it before it runs.
    setenv("Z42_TEST_JOBS", "1", 0);

    int rc = z42_host_run_app(app, ent, libs, (int)argc, argv);

    for (jsize i = 0; i < argc; i++) {
        (*env)->ReleaseStringUTFChars(env, jargs[i], argv[i]);
        (*env)->DeleteLocalRef(env, jargs[i]);
    }
    free(argv);
    free(jargs);
    (*env)->ReleaseStringUTFChars(env, appPath, app);
    if (ent)  (*env)->ReleaseStringUTFChars(env, entry, ent);
    if (libs) (*env)->ReleaseStringUTFChars(env, libsDir, libs);
    return (jint)rc;
}

/* ── z42_zpkg_read_namespaces bridge (static) ────────────────────────────
 *
 * Stateless: reads a zpkg's NSPC section so AssetZpkgResolver can build a
 * namespace → bytes map from the asset packages directly — no index.json.
 * The visitor collects C strings (JNIEnv-free); String[] is built after.
 */

typedef struct {
    char** items;
    size_t count;
    size_t cap;
} NsCollector;

static void ns_collect(const char* ns, size_t len, void* user_data) {
    NsCollector* c = (NsCollector*)user_data;
    if (c->count == c->cap) {
        size_t ncap = c->cap ? c->cap * 2 : 8;
        char** ni = (char**)realloc(c->items, ncap * sizeof(char*));
        if (!ni) return;
        c->items = ni;
        c->cap = ncap;
    }
    char* s = (char*)malloc(len + 1);
    if (!s) return;
    memcpy(s, ns, len);
    s[len] = '\0';
    c->items[c->count++] = s;
}

JNIEXPORT jobjectArray JNICALL
Java_io_z42_vm_Z42VM_readNamespaces(JNIEnv* env, jclass clazz, jbyteArray bytes) {
    (void)clazz;
    jsize len = (*env)->GetArrayLength(env, bytes);
    jbyte* buf = (*env)->GetByteArrayElements(env, bytes, NULL);

    NsCollector c = { NULL, 0, 0 };
    z42_zpkg_read_namespaces((const uint8_t*)buf, (size_t)len, ns_collect, &c);
    if (buf) (*env)->ReleaseByteArrayElements(env, bytes, buf, JNI_ABORT);

    jclass str_class = (*env)->FindClass(env, "java/lang/String");
    jobjectArray arr = (*env)->NewObjectArray(env, (jsize)c.count, str_class, NULL);
    for (size_t i = 0; i < c.count; i++) {
        jstring js = (*env)->NewStringUTF(env, c.items[i]);
        (*env)->SetObjectArrayElement(env, arr, (jsize)i, js);
        (*env)->DeleteLocalRef(env, js);
        free(c.items[i]);
    }
    free(c.items);
    return arr;
}
