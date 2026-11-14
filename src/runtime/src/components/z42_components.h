/* Same-build final-link composition contract; not the ordinary VM host API. */
#ifndef Z42_COMPONENTS_H
#define Z42_COMPONENTS_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define Z42_COMPONENT_OK 0u
#define Z42_COMPONENT_ERROR 2u
#define Z42_COMPONENT_INVALID_ARGUMENT 3u

/* Same-build component factory. The descriptor is opaque to C hosts. */
typedef struct Z42ComponentDescriptor Z42ComponentDescriptor;
typedef const Z42ComponentDescriptor* (*Z42ComponentInit)(void);
typedef struct Z42ComponentId { uint8_t bytes[16]; } Z42ComponentId;
static const Z42ComponentId Z42_COMPONENT_INTERP = {{0x7a, 0x34, 0x32, 0x2e, 0x69, 0x6e, 0x74, 0x65, 0x72, 0x70, 0x2e, 0x76, 0x31, 0x00, 0x00, 0x00}};
static const Z42ComponentId Z42_COMPONENT_JIT = {{0x7a, 0x34, 0x32, 0x2e, 0x6a, 0x69, 0x74, 0x2e, 0x76, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}};

static const Z42ComponentId Z42_COMPONENT_PROFILER = {{0x7a, 0x34, 0x32, 0x2e, 0x70, 0x72, 0x6f, 0x66, 0x69, 0x6c, 0x65, 0x72, 0x2e, 0x76, 0x31, 0x00}};

/* Factories are supplied by the selected component libraries, not the core. */
const Z42ComponentDescriptor* z42_interp_init(void);
const Z42ComponentDescriptor* z42_jit_init(void);
const Z42ComponentDescriptor* z42_profiler_init(void);

/* Call before creating a host. init and its callbacks must be trusted and remain
 * loaded for the process lifetime. Repeating the same registration is idempotent;
 * a null initializer returns Z42_COMPONENT_INVALID_ARGUMENT; a conflict returns
 * Z42_COMPONENT_ERROR. Registration never changes host error state.
 * Descriptor/build/ABI validation still occurs before a program is loaded. */
uint32_t z42_component_register_static(Z42ComponentId id, Z42ComponentInit init);

/* Final static archives register their bundled backends here. Dynamic cores
 * expose the same entry as a no-op and discover colocated components on load. */
int z42_linked_components_init(void);

#ifdef __cplusplus
}
#endif
#endif /* Z42_COMPONENTS_H */
