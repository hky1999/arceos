use axhal::{arch::TrapFrame, mem::VirtAddr, paging::MappingFlags};

struct TrapHandlerImpl;

#[crate_interface::impl_interface]
impl axhal::trap::TrapHandler for TrapHandlerImpl {
    // #[cfg(not(any(feature = "hv", target_arch = "aarch64")))]
    fn handle_irq(_irq_num: usize) {
        #[cfg(feature = "irq")]
        {
            let guard = kernel_guard::NoPreempt::new();
            axhal::irq::dispatch_irq(_irq_num);
            drop(guard); // rescheduling may occur when preemption is re-enabled.
        }
    }
    /*
    #[cfg(feature = "hv", target_arch = "aarch64")]
    fn handle_irq(_irq_num: usize) {
        #[cfg(feature = "irq")]
        {
            let guard = kernel_guard::NoPreempt::new();
            if _irq_num >= 0 && _irq_num <=15 {
                axhal::irq::dispatch_irq(_irq_num);
            } else if _irq_num >= 16 && _irq_num <=32 {
                //todo()
                interrupt_handler(_irq_num);
            }
            drop(guard);
        }
    }
    */
    #[cfg(feature = "monolithic")]
    fn handle_page_fault(addr: VirtAddr, flags: MappingFlags, _tf: &mut TrapFrame) {
        axprocess::handle_page_fault(addr, flags);
    }
}
