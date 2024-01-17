use super::utils::deal_result;

#[no_mangle]
pub fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {

	// debug!("[SYSCALL] {syscall_id}");

    // let ans = 0;

    // let ans = deal_result(ans);
    // axlog::info!("syscall: {} -> {}", syscall_id, ans);
    // ans
	0
}
