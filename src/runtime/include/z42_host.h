/* Unified native embedding header. All runtime C declarations live in include/;
 * packaging inlines the aggregated-only parts (host_api / compression / components)
 * into this umbrella, while z42_abi.h / z42_native.h stay sibling includes. */
#ifndef Z42_HOST_H
#define Z42_HOST_H
#include "z42_abi.h"
#include "z42_native.h"
#include "z42_compression.h"
#include "z42_host_api.h"
#include "z42_components.h"
#endif /* Z42_HOST_H */
