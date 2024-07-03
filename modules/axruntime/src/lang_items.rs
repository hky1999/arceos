use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // I add this line just to make sure 
    // we can see the output from panic
    // BEFORE logger is initialized.
    axlog::ax_println!("{}", info);
    error!("{}", info);
    axhal::misc::terminate()
}
