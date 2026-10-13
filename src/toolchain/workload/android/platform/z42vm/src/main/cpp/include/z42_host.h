/* Forwarding header — pulls in the canonical z42_host.h from
 * src/runtime/include/. The JNI bridge in z42vm_jni.c uses this for
 * Z42HostConfig + the z42_host_* function declarations.
 *
 * Spec: 变更 add-platform-android/
 */
#include "../../../../../../../../../runtime/include/z42_host.h"
