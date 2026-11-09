//! Borrowed call inputs; backends fill their own activation without an argument Vec.
use crate::object::Value;

pub enum CallArgs<'a> {
    Values(&'a [Value]),
    Registers { regs: &'a [Value], indices: &'a [u32] },
    Receiver { receiver: Value, regs: &'a [Value], indices: &'a [u32] },
}
