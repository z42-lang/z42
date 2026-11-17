#ifndef Z42_COMPRESSION_H
#define Z42_COMPRESSION_H
#ifdef __cplusplus
extern "C" {
#endif
/* Linked library resolver; registration remains lazy until a script uses it. */
const void* z42_compression_get_symbol(const char* name);
#ifdef __cplusplus
}
#endif
#endif
