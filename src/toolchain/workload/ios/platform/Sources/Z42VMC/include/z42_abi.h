/* Forwarding header — pulls in the canonical z42_abi.h from
 * src/runtime/include/. Swift's clang module sees the same types as
 * the runtime crate.
 *
 * Spec: 变更 add-platform-ios/
 */
#include "../../../../../../../runtime/include/z42_abi.h"
