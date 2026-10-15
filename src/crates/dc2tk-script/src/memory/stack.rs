//! A memory stack, counted as mgCMemory counts it.

use super::*;

/// Quadwords a load takes of an mgCMemory stack (mg_memory.cpp): Alloc adds its size, stAlloc64
/// and Align64 first round the top up to 64 bytes (the stacks start 64-byte aligned).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stack {
    pub used: u64,
}

impl Stack {
    pub(super) fn alloc(&mut self, quadwords: u64) {
        self.used += quadwords;
    }

    pub(super) fn align64(&mut self) {
        self.used = round_up(self.used, 4);
    }

    pub(super) fn alloc64(&mut self, quadwords: u64) {
        self.align64();
        self.alloc(quadwords);
    }

    /// Bytes used.
    pub fn bytes(&self) -> u64 {
        self.used * QW
    }
}

/// align16_blocks / algn16_size: quadwords that hold `bytes`.
pub(super) fn blocks(bytes: u64) -> u64 {
    bytes.div_ceil(QW)
}
