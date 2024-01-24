use axvm;

pub fn boot_linux(hart_id: usize) {
    axvm::linux(hart_id);
}