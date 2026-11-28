//! Minimal decoded fixtures shared by runtime tests.

pub(crate) fn empty_zbc() -> Vec<u8> {
    use crate::zpkg::format::*;
    let mut bytes = vec![0; 16];
    bytes[..4].copy_from_slice(&ZBC_MAGIC);
    bytes[4..6].copy_from_slice(&ZBC_VERSION_MAJOR.to_le_bytes());
    bytes[6..8].copy_from_slice(&ZBC_VERSION_MINOR.to_le_bytes());
    bytes
}
