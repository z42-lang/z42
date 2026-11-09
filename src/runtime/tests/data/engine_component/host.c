/* Real C consumers of independently packaged static/dynamic components. */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include "z42_host.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef Z42_COMPONENT_DYNAMIC
#ifdef _WIN32
#include <windows.h>
/* Core dependencies live beside the DLL, independently of the C executable. */
#define OPEN(path) LoadLibraryExA(path, NULL, LOAD_WITH_ALTERED_SEARCH_PATH)
#define SYMBOL(lib, name) GetProcAddress(lib, name)
#else
#include <dlfcn.h>
#define OPEN(path) dlopen(path, RTLD_NOW | RTLD_LOCAL)
#define SYMBOL(lib, name) dlsym(lib, name)
#endif
#endif

typedef Z42HostStatus (*Initialize)(const Z42HostConfig*, Z42HostRef*);
typedef Z42HostStatus (*Load)(Z42HostRef, const uint8_t*, size_t, Z42ModuleRef*);
typedef Z42HostStatus (*Resolve)(Z42HostRef, Z42ModuleRef, const char*, Z42EntryRef*);
typedef Z42HostStatus (*Invoke)(Z42EntryRef, const Z42Value*, size_t, Z42Value*);
typedef Z42HostStatus (*Shutdown)(Z42HostRef);
typedef Z42Error (*LastError)(Z42HostRef);
typedef Z42HostStatus (*Register)(Z42ComponentId, Z42ComponentInit);
#define REQUIRE(condition) do { if (!(condition)) { fprintf(stderr, "C host check failed at line %d: %s\n", __LINE__, #condition); return 1; } } while (0)
typedef Z42HostStatus (*RegisterNative)(const char*, Z42NativeSymbolResolver);
struct Api { RegisterNative native; Initialize initialize; Load load; Resolve resolve; Invoke invoke;
    Shutdown shutdown; LastError error; Register reg; };
static const Z42ComponentDescriptor* conflicting_factory(void) { return NULL; }
static const void* empty_resolver(const char* name) { (void)name; return NULL; }

#ifdef Z42_COMPONENT_DYNAMIC
static int module_loaded(const char* path) {
#ifdef _WIN32
    return GetModuleHandleA(path) != NULL;
#else
    void* module = dlopen(path, RTLD_NOW | RTLD_NOLOAD);
    if (!module) return 0;
    dlclose(module); return 1;
#endif
}
#endif
static int failure(struct Api* api, Z42HostRef host, int expect_missing) {
    Z42Error error = api->error(host);
    const char* message = error.message ? error.message : "no error message";
    fprintf(stderr, "component host: %s\n", message);
    int missing = expect_missing == 2 ? strstr(message, "compression") != NULL
        : expect_missing && (strstr(message, "JIT") || strstr(message, "jit") || strstr(message, "Jit"));
    if (host) api->shutdown(host);
    return missing ? 0 : 1;
}
int main(int argc, char** argv) {
    if (argc < 5) return 2; /* core path, bytecode, libs, mode, [expect-missing] */
    int expect_missing = argc > 5 && strcmp(argv[5], "expect-missing") == 0;
    struct Api api;
#ifdef Z42_COMPONENT_DYNAMIC
    /* Retain the core image through process/TLS teardown. */
#ifdef _WIN32
    HMODULE library = OPEN(argv[1]);
#else
    void* library = OPEN(argv[1]);
#endif
    if (!library) { fprintf(stderr, "cannot open core: %s\n", argv[1]); return 1; }
#define BIND(field, type, name) api.field = (type)SYMBOL(library, name); if (!api.field) return 1
    BIND(native, RegisterNative, "z42_host_register_static_native_library");
    BIND(initialize, Initialize, "z42_host_initialize");
    BIND(load, Load, "z42_host_load_zbc");
    BIND(resolve, Resolve, "z42_host_resolve_entry");
    BIND(invoke, Invoke, "z42_host_invoke");
    BIND(shutdown, Shutdown, "z42_host_shutdown");
    BIND(error, LastError, "z42_host_last_error");
    BIND(reg, Register, "z42_host_register_static_component");
    typedef int (*LinkedInit)(void);
    LinkedInit linked_init = (LinkedInit)SYMBOL(library, "z42_linked_components_init");
    if (!linked_init || linked_init() != 0) return 1;
#else
    api = (struct Api){ z42_host_register_static_native_library, z42_host_initialize, z42_host_load_zbc, z42_host_resolve_entry,
        z42_host_invoke, z42_host_shutdown, z42_host_last_error, z42_host_register_static_component };
    if (api.reg(Z42_COMPONENT_INTERP, NULL) != Z42_HOST_ERR_BAD_CONFIG) return 1;
    if (api.reg(Z42_COMPONENT_INTERP, z42_interp_init) != Z42_HOST_OK) return 1;
    if (api.reg(Z42_COMPONENT_INTERP, z42_interp_init) != Z42_HOST_OK) return 1;
    if (api.reg(Z42_COMPONENT_INTERP, conflicting_factory) != Z42_HOST_ERR_BAD_CONFIG) return 1;
#ifdef Z42_COMPONENT_HAS_JIT
    if (api.reg(Z42_COMPONENT_JIT, z42_jit_init) != Z42_HOST_OK) return 1;
#endif
#ifdef Z42_COMPONENT_HAS_COMPRESSION
    REQUIRE(api.native("z42_compression", z42_compression_get_symbol) == Z42_HOST_OK);
    REQUIRE(api.native("z42_compression", z42_compression_get_symbol) == Z42_HOST_OK);
    REQUIRE(api.native("z42_compression", empty_resolver) == Z42_HOST_ERR_BAD_CONFIG);
    REQUIRE(z42_compression_get_symbol(NULL) == NULL);
    REQUIRE(z42_compression_get_symbol("unknown_symbol") == NULL);
#endif
#endif
#ifdef Z42_COMPONENT_DYNAMIC
    if (argc > 6) REQUIRE(!module_loaded(argv[6]));
#endif
    REQUIRE(api.native(NULL, empty_resolver) == Z42_HOST_ERR_BAD_CONFIG);
    REQUIRE(api.native("z42_compression", NULL) == Z42_HOST_ERR_BAD_CONFIG);
    REQUIRE(api.native("unknown_library", empty_resolver) == Z42_HOST_ERR_BAD_CONFIG);
    const char* search[] = { argv[3], NULL };
    Z42HostConfig cfg = {0};
    cfg.abi_version = Z42_HOST_ABI_VERSION; cfg.struct_size = sizeof(cfg);
    cfg.exec_mode = strcmp(argv[4], "jit") == 0 ? Z42_EXEC_MODE_JIT : Z42_EXEC_MODE_INTERP;
    cfg.search_paths = search;
    Z42HostRef host = NULL;
    if (api.initialize(&cfg, &host) != Z42_HOST_OK) return failure(&api, host, expect_missing);
    FILE* file = fopen(argv[2], "rb");
    if (!file || fseek(file, 0, SEEK_END)) return 1;
    long length = ftell(file);
    if (length <= 0 || fseek(file, 0, SEEK_SET)) return 1;
    uint8_t* bytes = malloc((size_t)length);
    if (!bytes || fread(bytes, 1, (size_t)length, file) != (size_t)length) return 1;
    fclose(file);
    Z42ModuleRef module = NULL;
    if (api.load(host, bytes, (size_t)length, &module) != Z42_HOST_OK) return failure(&api, host, expect_missing);
    if (expect_missing) { fprintf(stderr, "unlinked JIT unexpectedly loaded\n"); return 1; }
    Z42EntryRef entry = NULL;
    if (api.resolve(host, module, "EngineComponentProbe.Echo", &entry) != Z42_HOST_OK) return failure(&api, host, 0);
    for (uint64_t i = 40; i < 43; ++i) {
        Z42Value arg = { Z42_VALUE_TAG_I64, 0, i }, result = {0};
        if (api.invoke(entry, &arg, 1, &result) != Z42_HOST_OK) return failure(&api, host, 0);
        REQUIRE(result.tag == Z42_VALUE_TAG_I64 && result.payload == i + (i - 39));
    }
#ifdef Z42_COMPONENT_DYNAMIC
    if (argc > 6) REQUIRE(!module_loaded(argv[6]));
#endif
    if (argc > 5 && strncmp(argv[5], "compression", 11) == 0) {
        REQUIRE(api.resolve(host, module, "EngineComponentProbe.CompressionRoundtrip", &entry) == Z42_HOST_OK);
        Z42Value result = {0};
        Z42HostStatus compressed = api.invoke(entry, NULL, 0, &result);
        int missing = strcmp(argv[5], "compression-missing") == 0;
        if (compressed != Z42_HOST_OK) return failure(&api, host, missing ? 2 : 0);
        REQUIRE(!missing && result.tag == Z42_VALUE_TAG_I64 && result.payload == 36);
#ifdef Z42_COMPONENT_DYNAMIC
        if (argc > 6) REQUIRE(module_loaded(argv[6]));
#endif
    }
    if (api.shutdown(host) != Z42_HOST_OK) return failure(&api, host, 0);
    free(bytes);
    puts("component C host: ok");
    return 0;
}
