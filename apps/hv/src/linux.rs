use axvm;

#[cfg(feature = "type1_5")]
pub fn boot_linux() {
    axvm::boot_host_linux();
}

#[cfg(not(feature = "type1_5"))]
pub fn boot_linux() {
    axvm::boot_host_linux();
    use axprocess;
    loop {
        // if unsafe { axprocess::wait_pid(now_process_id, &mut exit_code as *mut i32) }.is_ok() {
        //     break Some(exit_code);
        // }

        axprocess::yield_now_task();
    }
}
