//! Whole-value access to inline heap union bytes.
use super::Cells;
use anyhow::{ensure, Result};
use parking_lot::{Mutex, MutexGuard};

const LOCK_COUNT: usize = 64;
static LOCKS: [Mutex<()>; LOCK_COUNT] = [const { Mutex::new(()) }; LOCK_COUNT];

pub(crate) fn is_storage_name(name: &str) -> bool {
    name.starts_with("$union.")
}

pub(crate) fn read(cells: &Cells, offset: usize, bytes: &mut [u8]) -> Result<()> {
    let _guard = lock(cells, offset, bytes.len());
    ensure!(cells.copy_out(offset, bytes), "inline union read exceeds storage");
    Ok(())
}

pub(crate) fn write(cells: &Cells, offset: usize, bytes: &[u8]) -> Result<()> {
    let _guard = lock(cells, offset, bytes.len());
    ensure!(cells.copy_in(offset, bytes), "inline union write exceeds storage");
    Ok(())
}

pub(crate) fn lock(cells: &Cells, offset: usize, size: usize) -> Option<MutexGuard<'static, ()>> {
    // Contiguous equal-width single-cell accesses are already atomic. All other
    // accesses to the same union use the same shard, regardless of its owner.
    if size <= 8 && size.is_power_of_two() && offset % size == 0 {
        return None;
    }
    let address = cells.base_ptr() as usize + offset;
    Some(LOCKS[((address >> 3) ^ (address >> 12)) % LOCK_COUNT].lock())
}
