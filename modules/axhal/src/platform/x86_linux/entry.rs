use super::percpu::PerCpu;

unsafe extern "sysv64" fn switch_stack(linux_sp: usize) -> i32 {
    let linux_tp = x86::msr::rdmsr(x86::msr::IA32_GS_BASE);
    let cpu_data = PerCpu::new();
    let hv_sp = cpu_data.stack_top();
    let ret;
    core::arch::asm!("
        mov [rsi], {linux_tp}   // save gs_base to stack
        mov rcx, rsp
        mov rsp, {hv_sp}
        push rcx
        call {entry}
        pop rsp",
        entry = sym super::vmm_cpu_entry,
        linux_tp = in(reg) linux_tp,
        hv_sp = in(reg) hv_sp,
        in("rdi") cpu_data,
        in("rsi") linux_sp,
        lateout("rax") ret,
        out("rcx") _,
        clobber_abi("sysv64"),
    );
    x86::msr::wrmsr(x86::msr::IA32_GS_BASE, linux_tp);
    ret
}

#[naked]
#[no_mangle]
pub unsafe extern "C" fn _start() -> i32 {
    core::arch::asm!("
        // rip is pushed
        cli
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15
        push 0  // skip gs_base

        mov rdi, rsp
        call {0}

        pop r15 // skip gs_base
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp
        ret
        // rip will pop when return",
        sym switch_stack,
        options(noreturn),
    );
}
