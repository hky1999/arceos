use axvm;
use axprocess;

pub fn boot_linux(hart_id: usize) {
    axvm::config_linux(hart_id);

    loop {
        // if unsafe { axprocess::wait_pid(now_process_id, &mut exit_code as *mut i32) }.is_ok() {
        //     break Some(exit_code);
        // }

        axprocess::yield_now_task();
    };
}