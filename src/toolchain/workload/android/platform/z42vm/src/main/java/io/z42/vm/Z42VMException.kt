package io.z42.vm

/**
 * Thrown for any non-OK status returned by the runtime. `status` is
 * the numeric [Z42HostStatus](../../../../../../../runtime/include/z42_host.h)
 * value (1..99); `message` is the runtime's diagnostic.
 *
 * Spec: docs/spec/archive/2026-05-12-add-platform-android/
 */
class Z42VMException(
    @JvmField val status: Int,
    message: String,
) : RuntimeException("Z42VMError($status): $message") {

    companion object {
        const val ALREADY_INIT    = 1
        const val NOT_INIT        = 2
        const val BAD_CONFIG      = 3
        const val FEATURE_OFF     = 4
        const val BAD_ZBC         = 10
        const val VERIFICATION    = 11
        const val ENTRY_NOT_FOUND = 20
        const val ARG_MISMATCH    = 21
        const val VM_EXCEPTION    = 30
        const val FATAL           = 31   // fatal VM error (stack overflow); shut the VM down
        const val INTERNAL        = 99
    }
}
