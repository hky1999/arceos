use axhal::arch::TrapFrame;

pub fn syscall(
    tf: &mut TrapFrame,
    syscall_id: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
) -> isize {
    axhal::arch::enable_irqs();
    // debug!(
    //     "syscall {} enter <= ({:#x}, {:#x}, {:#x})",
    //     syscall_id, arg0, arg1, arg2
    // );
    // let ret = match syscall_id {
    //     SYSCALL_READ => sys_read(arg0, arg1.into(), arg2),
    //     SYSCALL_WRITE => sys_write(arg0, arg1.into(), arg2),
    //     SYSCALL_YIELD => sys_yield(),
    //     SYSCALL_NANOSLEEP => sys_nanosleep(arg0.into()),
    //     SYSCALL_GETPID => sys_getpid(),
    //     SYSCALL_CLONE => sys_clone(arg0, tf),
    //     SYSCALL_FORK => sys_fork(tf),
    //     SYSCALL_EXEC => sys_exec(arg0.into(), tf),
    //     SYSCALL_EXIT => sys_exit(arg0 as i32),
    //     SYSCALL_WAITPID => sys_waitpid(arg0 as isize, arg1.into()),
    //     SYSCALL_GET_TIME_MS => sys_get_time_ms(),
    //     SYSCALL_CLOCK_GETTIME => sys_clock_gettime(arg0, arg1.into()),
    //     _ => {
    //         println!("Unsupported syscall_id: {}", syscall_id);
    //         crate::task::CurrentTask::get().exit(-1);
    //     }
    // };
    debug!("syscall {} ret => {:#x}", syscall_id, ret);
    axhal::arch::disable_irqs();
    ret
}
