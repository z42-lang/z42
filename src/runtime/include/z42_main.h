#ifndef Z42_MAIN_H
#define Z42_MAIN_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* cli-enabled Core process entry, called once by z42vm after loading this library.
 * Reads this process's OS arguments and environment; returns its exit status.
 * Does not provide an embedding API. */
int32_t z42_main_v1(void);
#ifdef __cplusplus
}
#endif
#endif
