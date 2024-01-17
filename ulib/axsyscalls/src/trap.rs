struct SyscallHandlerImpl;

#[crate_interface::impl_interface]
impl axhal::trap::SyscallHandler for SyscallHandlerImpl {
    fn handle_syscall(syscall_id: usize, args: [usize; 6]) -> isize {
        crate::syscall::syscall(syscall_id, args)
    }
}
