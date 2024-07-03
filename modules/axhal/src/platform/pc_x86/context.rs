use lazy_init::LazyInit;

use hypercraft::LinuxContext;

use crate::current_cpu_id;
use axconfig::SMP;

// #[percpu::def_percpu]
// static LINUX_CTX: LazyInit<LinuxContext> = LazyInit::new();

// const ARRAY_REPEAT_VALUE: LazyInit<LinuxContext> = ;
static mut LINUX_CTXS: [LazyInit<LinuxContext>; SMP] = [const { LazyInit::new() }; SMP];

pub fn save_linux_context(cpu_id: usize, linux_sp: usize) {
    // let linux_ctx = unsafe { LINUX_CTX.current_ref_mut_raw() };
    assert!(cpu_id < SMP, "illegal cpu_id which exceeds SMP number");
    unsafe {
        LINUX_CTXS[cpu_id].init_by({
            let mut ctx = LinuxContext::invalid();
            ctx.load_from(linux_sp);
            ctx
        });
    }
}

pub fn get_linux_context(cpu_id: usize) -> &'static LinuxContext {
    // unsafe { LINUX_CTX.current_ref_raw() }.try_get().unwrap()
    assert!(cpu_id < SMP, "illegal cpu_id which exceeds SMP number");
    unsafe { LINUX_CTXS[cpu_id].try_get().unwrap() }
}
