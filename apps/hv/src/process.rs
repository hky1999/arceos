use axprocess;

pub fn hello() {
    let main_task = axprocess::Process::init_hello().unwrap();
    let now_process_id = main_task.get_process_id() as isize;

    println!("New hello process id {}", now_process_id);

	let mut ans = None;

    let mut exit_code = 0;
    ans = loop {
        if unsafe { axprocess::wait_pid(now_process_id, &mut exit_code as *mut i32) }.is_ok() {
            break Some(exit_code);
        }

        axprocess::yield_now_task();
    };
}
