/* Unified native embedding header. All runtime C declarations live in include/;
 * packaging inlines the aggregated-only parts (native / host_api / compression /
 * components) into this umbrella, leaving z42_abi.h as the only sibling include. */
#ifndef Z42_HOST_H
#define Z42_HOST_H
#include "z42_abi.h"
#include "z42_native.h"
#include "z42_compression.h"
#include "z42_host_api.h"
#include "z42_components.h"
#endif /* Z42_HOST_H */
