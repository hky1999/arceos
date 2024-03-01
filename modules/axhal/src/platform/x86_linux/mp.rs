use core::sync::atomic::Ordering;

/// Starts the given secondary CPU with its boot stack.
pub fn start_secondary_cpu(_apic_id: usize, _stack_top: crate::mem::PhysAddr) {
   // No need
   // This step is completed by Linux.
   super::ARCEOS_MAIN_INIT_OK.store(1, Ordering::Release);
}

/// Starts the given secondary CPU with its boot stack.
pub fn start_arceos_cpu(_apic_id: usize, _stack_top: crate::mem::PhysAddr) {

}
