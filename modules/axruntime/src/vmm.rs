use core::sync::atomic::Ordering;

fn is_init_ok() -> bool {
    super::INITED_CPUS.load(Ordering::Acquire) == (axconfig::SMP)
}

/// The main entry point of the ArceOS runtime for secondary CPUs.
/// When booting from Linux and set self as VMM!
///
/// It is called from the `vmm_cpu_entry` code in [axhal].
#[no_mangle]
pub extern "C" fn rust_vmm_main_secondary(cpu_id: usize) {
    info!("Secondary CPU {:x} started.", cpu_id);

    info!("Secondary CPU {:x} init OK.", cpu_id);
    super::INITED_CPUS.fetch_add(1, Ordering::Relaxed);

    while !is_init_ok() {
        core::hint::spin_loop();
    }
}

/// The main entry point of the ArceOS runtime.
/// When booting from Linux and set self as VMM!
///
/// It is called from the `vmm_cpu_entry` code in [axhal]. `cpu_id` is the ID of
/// the current CPU, and `dtb` is the address of the device tree blob. It
/// finally calls the application's `main` function after all initialization
/// work is done.
///
/// In multi-core environment, this function is called on the primary CPU,
/// and the secondary CPUs call [`rust_vmm_main_secondary`].
#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn rust_vmm_main(cpu_id: usize) {
    ax_println!("{}", super::LOGO);
    ax_println!(
        "\
        arch = {}\n\
        platform = {}\n\
        target = {}\n\
        smp = {}\n\
        build_mode = {}\n\
        log_level = {}\n\
        ",
        option_env!("AX_ARCH").unwrap_or(""),
        option_env!("AX_PLATFORM").unwrap_or(""),
        option_env!("AX_TARGET").unwrap_or(""),
        option_env!("AX_SMP").unwrap_or(""),
        option_env!("AX_MODE").unwrap_or(""),
        option_env!("AX_LOG").unwrap_or(""),
    );

    axlog::init();
    axlog::set_max_level(option_env!("AX_LOG").unwrap_or("")); // no effect if set `log-level-*` features
    info!("Logging is enabled.");
    info!("VMM Primary CPU {} started", cpu_id);

    info!("Found physcial memory regions:");
    for r in axhal::mem::memory_regions() {
        info!(
            "  [{:x?}, {:x?}) {} ({:?})",
            r.paddr,
            r.paddr + r.size,
            r.name,
            r.flags
        );
    }

    super::init_allocator();

    axhal::mp::continue_secondary_cpus();

    info!("VMM Primary CPU {} init OK.", cpu_id);
    super::INITED_CPUS.fetch_add(1, Ordering::Relaxed);

    while !is_init_ok() {
        core::hint::spin_loop();
    }
}
