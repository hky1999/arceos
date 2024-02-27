/// Shutdown the whole system (in QEMU), including all CPUs.
///
/// See <https://wiki.osdev.org/Shutdown> for more information.
pub fn terminate() -> ! {
    info!("Shutting down...");

    crate::arch::halt();
    warn!("It should shutdown!");
    loop {
        crate::arch::halt();
    }
}
