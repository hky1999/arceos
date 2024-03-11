#![feature(prelude_import)]
//! [ArceOS] hardware abstraction layer, provides unified APIs for
//! platform-specific operations.
//!
//! It does the bootstrapping and initialization process for the specified
//! platform, and provides useful operations on the hardware.
//!
//! Currently supported platforms (specify by cargo features):
//!
//! - `x86-pc`: Standard PC with x86_64 ISA.
//! - `riscv64-qemu-virt`: QEMU virt machine with RISC-V ISA.
//! - `aarch64-qemu-virt`: QEMU virt machine with AArch64 ISA.
//! - `aarch64-raspi`: Raspberry Pi with AArch64 ISA.
//! - `dummy`: If none of the above platform is selected, the dummy platform
//!    will be used. In this platform, most of the operations are no-op or
//!    `unimplemented!()`. This platform is mainly used for [cargo test].
//!
//! # Cargo Features
//!
//! - `smp`: Enable SMP (symmetric multiprocessing) support.
//! - `fp_simd`: Enable floating-point and SIMD support.
//! - `paging`: Enable page table manipulation.
//! - `irq`: Enable interrupt handling support.
//!
//! [ArceOS]: https://github.com/rcore-os/arceos
//! [cargo test]: https://doc.rust-lang.org/cargo/guide/tests.html
#![no_std]
#![feature(asm_const)]
#![feature(naked_functions)]
#![feature(const_option)]
#![feature(doc_auto_cfg)]
#[prelude_import]
use core::prelude::rust_2021::*;
#[macro_use]
extern crate core;
extern crate compiler_builtins as _;
#[allow(unused_imports)]
#[macro_use]
extern crate log;
mod platform {
    //! Platform-specific operations.
    mod x86_linux {
        mod apic {
            #![allow(dead_code)]
            use lazy_init::LazyInit;
            use memory_addr::PhysAddr;
            use spinlock::SpinNoIrq;
            use x2apic::ioapic::IoApic;
            use x2apic::lapic::{xapic_base, LocalApic, LocalApicBuilder};
            use x86_64::instructions::port::Port;
            use self::vectors::*;
            use crate::mem::phys_to_virt;
            pub(super) mod vectors {
                pub const APIC_TIMER_VECTOR: u8 = 0xf0;
                pub const APIC_SPURIOUS_VECTOR: u8 = 0xf1;
                pub const APIC_ERROR_VECTOR: u8 = 0xf2;
            }
            /// The maximum number of IRQs.
            pub const MAX_IRQ_COUNT: usize = 256;
            /// The timer IRQ number.
            pub const TIMER_IRQ_NUM: usize = APIC_TIMER_VECTOR as usize;
            const IO_APIC_BASE: PhysAddr = PhysAddr::from(0xFEC0_0000);
            static mut LOCAL_APIC: Option<LocalApic> = None;
            static mut IS_X2APIC: bool = false;
            static IO_APIC: LazyInit<SpinNoIrq<IoApic>> = LazyInit::new();
            pub(super) fn local_apic<'a>() -> &'a mut LocalApic {
                unsafe { LOCAL_APIC.as_mut().unwrap() }
            }
            pub(super) fn raw_apic_id(id_u8: u8) -> u32 {
                if unsafe { IS_X2APIC } { id_u8 as u32 } else { (id_u8 as u32) << 24 }
            }
            fn cpu_has_x2apic() -> bool {
                match raw_cpuid::CpuId::new().get_feature_info() {
                    Some(finfo) => finfo.has_x2apic(),
                    None => false,
                }
            }
            pub(super) fn init_primary() {
                {
                    let lvl = ::log::Level::Info;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!("Initialize Local APIC..."),
                            lvl,
                            &(
                                "axhal::platform::x86_linux::apic",
                                "axhal::platform::x86_linux::apic",
                                "modules/axhal/src/platform/x86_linux/apic.rs",
                                87u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                unsafe {
                    Port::<u8>::new(0x21).write(0xff);
                    Port::<u8>::new(0xA1).write(0xff);
                }
                let mut builder = LocalApicBuilder::new();
                builder
                    .timer_vector(APIC_TIMER_VECTOR as _)
                    .error_vector(APIC_ERROR_VECTOR as _)
                    .spurious_vector(APIC_SPURIOUS_VECTOR as _);
                if cpu_has_x2apic() {
                    {
                        let lvl = ::log::Level::Info;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api_log(
                                format_args!("Using x2APIC."),
                                lvl,
                                &(
                                    "axhal::platform::x86_linux::apic",
                                    "axhal::platform::x86_linux::apic",
                                    "modules/axhal/src/platform/x86_linux/apic.rs",
                                    102u32,
                                ),
                                ::log::__private_api::Option::None,
                            );
                        }
                    };
                    unsafe { IS_X2APIC = true };
                } else {
                    {
                        let lvl = ::log::Level::Info;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api_log(
                                format_args!("Using xAPIC."),
                                lvl,
                                &(
                                    "axhal::platform::x86_linux::apic",
                                    "axhal::platform::x86_linux::apic",
                                    "modules/axhal/src/platform/x86_linux/apic.rs",
                                    105u32,
                                ),
                                ::log::__private_api::Option::None,
                            );
                        }
                    };
                    let base_vaddr = phys_to_virt(
                        PhysAddr::from(unsafe { xapic_base() } as usize),
                    );
                    builder.set_xapic_base(base_vaddr.as_usize() as u64);
                }
                let mut lapic = builder.build().unwrap();
                unsafe {
                    lapic.enable();
                    LOCAL_APIC = Some(lapic);
                }
                {
                    let lvl = ::log::Level::Info;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!("Initialize IO APIC..."),
                            lvl,
                            &(
                                "axhal::platform::x86_linux::apic",
                                "axhal::platform::x86_linux::apic",
                                "modules/axhal/src/platform/x86_linux/apic.rs",
                                116u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                let io_apic = unsafe {
                    IoApic::new(phys_to_virt(IO_APIC_BASE).as_usize() as u64)
                };
                IO_APIC.init_by(SpinNoIrq::new(io_apic));
            }
        }
        mod dtables {
            //! Description tables (per-CPU GDT, per-CPU ISS, IDT)
            use crate::arch::{GdtStruct, IdtStruct, TaskStateSegment};
            use lazy_init::LazyInit;
            static IDT: LazyInit<IdtStruct> = LazyInit::new();
            #[link_section = ".percpu"]
            static mut __PERCPU_TSS: LazyInit<TaskStateSegment> = LazyInit::new();
            ///Wrapper struct for the per-CPU data [`TSS`]
            #[allow(non_camel_case_types)]
            struct TSS_WRAPPER {}
            static TSS: TSS_WRAPPER = TSS_WRAPPER {};
            impl TSS_WRAPPER {
                /// Returns the offset relative to the per-CPU data area base on the current CPU.
                #[inline]
                pub fn offset(&self) -> usize {
                    let value: usize;
                    unsafe {
                        asm!("movabs {0}, offset {1}", out(reg) value, sym __PERCPU_TSS);
                    }
                    value
                }
                /// Returns the raw pointer of this per-CPU data on the current CPU.
                ///
                /// # Safety
                ///
                /// Caller must ensure that preemption is disabled on the current CPU.
                #[inline]
                pub unsafe fn current_ptr(&self) -> *const LazyInit<TaskStateSegment> {
                    #[cfg(not(target_os = "macos"))]
                    {
                        let base: usize;
                        #[cfg(target_arch = "x86_64")]
                        {
                            asm!(
                                "mov {0}, gs:[offset __PERCPU_SELF_PTR]\nadd {0}, offset {1}",
                                out(reg) base, sym __PERCPU_TSS
                            );
                            base as *const LazyInit<TaskStateSegment>
                        }
                    }
                }
                /// Returns the reference of the per-CPU data on the current CPU.
                ///
                /// # Safety
                ///
                /// Caller must ensure that preemption is disabled on the current CPU.
                #[inline]
                pub unsafe fn current_ref_raw(&self) -> &LazyInit<TaskStateSegment> {
                    &*self.current_ptr()
                }
                /// Returns the mutable reference of the per-CPU data on the current CPU.
                ///
                /// # Safety
                ///
                /// Caller must ensure that preemption is disabled on the current CPU.
                #[inline]
                #[allow(clippy::mut_from_ref)]
                pub unsafe fn current_ref_mut_raw(
                    &self,
                ) -> &mut LazyInit<TaskStateSegment> {
                    &mut *(self.current_ptr() as *mut LazyInit<TaskStateSegment>)
                }
                /// Manipulate the per-CPU data on the current CPU in the given closure.
                /// Preemption will be disabled during the call.
                pub fn with_current<F, T>(&self, f: F) -> T
                where
                    F: FnOnce(&mut LazyInit<TaskStateSegment>) -> T,
                {
                    f(unsafe { self.current_ref_mut_raw() })
                }
            }
            #[link_section = ".percpu"]
            static mut __PERCPU_GDT: LazyInit<GdtStruct> = LazyInit::new();
            ///Wrapper struct for the per-CPU data [`GDT`]
            #[allow(non_camel_case_types)]
            struct GDT_WRAPPER {}
            static GDT: GDT_WRAPPER = GDT_WRAPPER {};
            impl GDT_WRAPPER {
                /// Returns the offset relative to the per-CPU data area base on the current CPU.
                #[inline]
                pub fn offset(&self) -> usize {
                    let value: usize;
                    unsafe {
                        asm!("movabs {0}, offset {1}", out(reg) value, sym __PERCPU_GDT);
                    }
                    value
                }
                /// Returns the raw pointer of this per-CPU data on the current CPU.
                ///
                /// # Safety
                ///
                /// Caller must ensure that preemption is disabled on the current CPU.
                #[inline]
                pub unsafe fn current_ptr(&self) -> *const LazyInit<GdtStruct> {
                    #[cfg(not(target_os = "macos"))]
                    {
                        let base: usize;
                        #[cfg(target_arch = "x86_64")]
                        {
                            asm!(
                                "mov {0}, gs:[offset __PERCPU_SELF_PTR]\nadd {0}, offset {1}",
                                out(reg) base, sym __PERCPU_GDT
                            );
                            base as *const LazyInit<GdtStruct>
                        }
                    }
                }
                /// Returns the reference of the per-CPU data on the current CPU.
                ///
                /// # Safety
                ///
                /// Caller must ensure that preemption is disabled on the current CPU.
                #[inline]
                pub unsafe fn current_ref_raw(&self) -> &LazyInit<GdtStruct> {
                    &*self.current_ptr()
                }
                /// Returns the mutable reference of the per-CPU data on the current CPU.
                ///
                /// # Safety
                ///
                /// Caller must ensure that preemption is disabled on the current CPU.
                #[inline]
                #[allow(clippy::mut_from_ref)]
                pub unsafe fn current_ref_mut_raw(&self) -> &mut LazyInit<GdtStruct> {
                    &mut *(self.current_ptr() as *mut LazyInit<GdtStruct>)
                }
                /// Manipulate the per-CPU data on the current CPU in the given closure.
                /// Preemption will be disabled during the call.
                pub fn with_current<F, T>(&self, f: F) -> T
                where
                    F: FnOnce(&mut LazyInit<GdtStruct>) -> T,
                {
                    f(unsafe { self.current_ref_mut_raw() })
                }
            }
            fn init_percpu() {
                unsafe {
                    IDT.load();
                    let tss = TSS.current_ref_mut_raw();
                    let gdt = GDT.current_ref_mut_raw();
                    tss.init_by(TaskStateSegment::new());
                    gdt.init_by(GdtStruct::new(tss));
                    gdt.load();
                    gdt.load_tss();
                }
            }
            /// Initializes IDT, GDT on the primary CPU.
            pub(super) fn init_primary() {
                ::axlog::__print_impl(
                    format_args!("{0}\n", format_args!("\nInitialize IDT & GDT...")),
                );
                IDT.init_by(IdtStruct::new());
                init_percpu();
            }
        }
        mod entry {
            use super::percpu::PerCpu;
            unsafe extern "sysv64" fn switch_stack(linux_sp: usize) -> i32 {
                let linux_tp = x86::msr::rdmsr(x86::msr::IA32_GS_BASE);
                let cpu_data = PerCpu::new();
                let hv_sp = cpu_data.stack_top();
                let ret;
                asm!(
                    "\n        mov [rsi], {1}   // save gs_base to stack\n        mov rcx, rsp\n        mov rsp, {2}\n        push rcx\n        call {0}\n        pop rsp",
                    sym super::vmm_cpu_entry, in (reg) linux_tp, in (reg) hv_sp, in
                    ("rdi") cpu_data, in ("rsi") linux_sp, in ("rdx") linux_tp,
                    lateout("rax") ret, out("rcx") _
                );
                ret
            }
            #[naked]
            #[no_mangle]
            pub unsafe extern "C" fn _start() -> i32 {
                asm!(
                    "\n        // rip is pushed\n        cli\n        push rbp\n        push rbx\n        push r12\n        push r13\n        push r14\n        push r15\n        push 0  // skip gs_base\n\n        mov rdi, rsp\n        call {0}\n\n        pop r15 // skip gs_base\n        pop r15\n        pop r14\n        pop r13\n        pop r12\n        pop rbx\n        pop rbp\n        ret\n        // rip will pop when return",
                    sym switch_stack, options(noreturn)
                );
            }
        }
        mod uart16550 {
            //! Uart 16550.
            use spinlock::SpinNoIrq;
            use x86_64::instructions::port::{Port, PortReadOnly, PortWriteOnly};
            const UART_CLOCK_FACTOR: usize = 16;
            const OSC_FREQ: usize = 1_843_200;
            static COM1: SpinNoIrq<Uart16550> = SpinNoIrq::new(Uart16550::new(0x3f8));
            /// Line status flags
            struct LineStsFlags(
                <LineStsFlags as ::bitflags::__private::PublicFlags>::Internal,
            );
            impl LineStsFlags {
                #[allow(deprecated, non_upper_case_globals)]
                pub const INPUT_FULL: Self = Self::from_bits_retain(1);
                #[allow(deprecated, non_upper_case_globals)]
                pub const OUTPUT_EMPTY: Self = Self::from_bits_retain(1 << 5);
            }
            impl ::bitflags::Flags for LineStsFlags {
                const FLAGS: &'static [::bitflags::Flag<LineStsFlags>] = &[
                    {
                        #[allow(deprecated, non_upper_case_globals)]
                        ::bitflags::Flag::new("INPUT_FULL", LineStsFlags::INPUT_FULL)
                    },
                    {
                        #[allow(deprecated, non_upper_case_globals)]
                        ::bitflags::Flag::new("OUTPUT_EMPTY", LineStsFlags::OUTPUT_EMPTY)
                    },
                ];
                type Bits = u8;
                fn bits(&self) -> u8 {
                    LineStsFlags::bits(self)
                }
                fn from_bits_retain(bits: u8) -> LineStsFlags {
                    LineStsFlags::from_bits_retain(bits)
                }
            }
            #[allow(
                dead_code,
                deprecated,
                unused_doc_comments,
                unused_attributes,
                unused_mut,
                unused_imports,
                non_upper_case_globals,
                clippy::assign_op_pattern,
                clippy::indexing_slicing,
                clippy::same_name_method
            )]
            const _: () = {
                #[repr(transparent)]
                struct InternalBitFlags(u8);
                #[automatically_derived]
                impl ::core::clone::Clone for InternalBitFlags {
                    #[inline]
                    fn clone(&self) -> InternalBitFlags {
                        let _: ::core::clone::AssertParamIsClone<u8>;
                        *self
                    }
                }
                #[automatically_derived]
                impl ::core::marker::Copy for InternalBitFlags {}
                #[automatically_derived]
                impl ::core::marker::StructuralPartialEq for InternalBitFlags {}
                #[automatically_derived]
                impl ::core::cmp::PartialEq for InternalBitFlags {
                    #[inline]
                    fn eq(&self, other: &InternalBitFlags) -> bool {
                        self.0 == other.0
                    }
                }
                #[automatically_derived]
                impl ::core::marker::StructuralEq for InternalBitFlags {}
                #[automatically_derived]
                impl ::core::cmp::Eq for InternalBitFlags {
                    #[inline]
                    #[doc(hidden)]
                    #[coverage(off)]
                    fn assert_receiver_is_total_eq(&self) -> () {
                        let _: ::core::cmp::AssertParamIsEq<u8>;
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::PartialOrd for InternalBitFlags {
                    #[inline]
                    fn partial_cmp(
                        &self,
                        other: &InternalBitFlags,
                    ) -> ::core::option::Option<::core::cmp::Ordering> {
                        ::core::cmp::PartialOrd::partial_cmp(&self.0, &other.0)
                    }
                }
                #[automatically_derived]
                impl ::core::cmp::Ord for InternalBitFlags {
                    #[inline]
                    fn cmp(&self, other: &InternalBitFlags) -> ::core::cmp::Ordering {
                        ::core::cmp::Ord::cmp(&self.0, &other.0)
                    }
                }
                #[automatically_derived]
                impl ::core::hash::Hash for InternalBitFlags {
                    #[inline]
                    fn hash<__H: ::core::hash::Hasher>(&self, state: &mut __H) -> () {
                        ::core::hash::Hash::hash(&self.0, state)
                    }
                }
                impl ::bitflags::__private::PublicFlags for LineStsFlags {
                    type Primitive = u8;
                    type Internal = InternalBitFlags;
                }
                impl ::bitflags::__private::core::default::Default for InternalBitFlags {
                    #[inline]
                    fn default() -> Self {
                        InternalBitFlags::empty()
                    }
                }
                impl ::bitflags::__private::core::fmt::Debug for InternalBitFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter<'_>,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        if self.is_empty() {
                            f.write_fmt(
                                format_args!("{0:#x}", <u8 as ::bitflags::Bits>::EMPTY),
                            )
                        } else {
                            ::bitflags::__private::core::fmt::Display::fmt(self, f)
                        }
                    }
                }
                impl ::bitflags::__private::core::fmt::Display for InternalBitFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter<'_>,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::parser::to_writer(&LineStsFlags(*self), f)
                    }
                }
                impl ::bitflags::__private::core::str::FromStr for InternalBitFlags {
                    type Err = ::bitflags::parser::ParseError;
                    fn from_str(
                        s: &str,
                    ) -> ::bitflags::__private::core::result::Result<Self, Self::Err> {
                        ::bitflags::parser::from_str::<LineStsFlags>(s)
                            .map(|flags| flags.0)
                    }
                }
                impl ::bitflags::__private::core::convert::AsRef<u8>
                for InternalBitFlags {
                    fn as_ref(&self) -> &u8 {
                        &self.0
                    }
                }
                impl ::bitflags::__private::core::convert::From<u8>
                for InternalBitFlags {
                    fn from(bits: u8) -> Self {
                        Self::from_bits_retain(bits)
                    }
                }
                #[allow(dead_code, deprecated, unused_attributes)]
                impl InternalBitFlags {
                    /// Get a flags value with all bits unset.
                    #[inline]
                    pub const fn empty() -> Self {
                        { Self(<u8 as ::bitflags::Bits>::EMPTY) }
                    }
                    /// Get a flags value with all known bits set.
                    #[inline]
                    pub const fn all() -> Self {
                        {
                            let mut truncated = <u8 as ::bitflags::Bits>::EMPTY;
                            let mut i = 0;
                            {
                                {
                                    let flag = <LineStsFlags as ::bitflags::Flags>::FLAGS[i]
                                        .value()
                                        .bits();
                                    truncated = truncated | flag;
                                    i += 1;
                                }
                            };
                            {
                                {
                                    let flag = <LineStsFlags as ::bitflags::Flags>::FLAGS[i]
                                        .value()
                                        .bits();
                                    truncated = truncated | flag;
                                    i += 1;
                                }
                            };
                            let _ = i;
                            Self::from_bits_retain(truncated)
                        }
                    }
                    /// Get the underlying bits value.
                    ///
                    /// The returned value is exactly the bits set in this flags value.
                    #[inline]
                    pub const fn bits(&self) -> u8 {
                        let f = self;
                        { f.0 }
                    }
                    /// Convert from a bits value.
                    ///
                    /// This method will return `None` if any unknown bits are set.
                    #[inline]
                    pub const fn from_bits(
                        bits: u8,
                    ) -> ::bitflags::__private::core::option::Option<Self> {
                        let bits = bits;
                        {
                            let truncated = Self::from_bits_truncate(bits).0;
                            if truncated == bits {
                                ::bitflags::__private::core::option::Option::Some(
                                    Self(bits),
                                )
                            } else {
                                ::bitflags::__private::core::option::Option::None
                            }
                        }
                    }
                    /// Convert from a bits value, unsetting any unknown bits.
                    #[inline]
                    pub const fn from_bits_truncate(bits: u8) -> Self {
                        let bits = bits;
                        { Self(bits & Self::all().bits()) }
                    }
                    /// Convert from a bits value exactly.
                    #[inline]
                    pub const fn from_bits_retain(bits: u8) -> Self {
                        let bits = bits;
                        { Self(bits) }
                    }
                    /// Get a flags value with the bits of a flag with the given name set.
                    ///
                    /// This method will return `None` if `name` is empty or doesn't
                    /// correspond to any named flag.
                    #[inline]
                    pub fn from_name(
                        name: &str,
                    ) -> ::bitflags::__private::core::option::Option<Self> {
                        let name = name;
                        {
                            {
                                if name == "INPUT_FULL" {
                                    return ::bitflags::__private::core::option::Option::Some(
                                        Self(LineStsFlags::INPUT_FULL.bits()),
                                    );
                                }
                            };
                            {
                                if name == "OUTPUT_EMPTY" {
                                    return ::bitflags::__private::core::option::Option::Some(
                                        Self(LineStsFlags::OUTPUT_EMPTY.bits()),
                                    );
                                }
                            };
                            let _ = name;
                            ::bitflags::__private::core::option::Option::None
                        }
                    }
                    /// Whether all bits in this flags value are unset.
                    #[inline]
                    pub const fn is_empty(&self) -> bool {
                        let f = self;
                        { f.bits() == <u8 as ::bitflags::Bits>::EMPTY }
                    }
                    /// Whether all known bits in this flags value are set.
                    #[inline]
                    pub const fn is_all(&self) -> bool {
                        let f = self;
                        { Self::all().bits() | f.bits() == f.bits() }
                    }
                    /// Whether any set bits in a source flags value are also set in a target flags value.
                    #[inline]
                    pub const fn intersects(&self, other: Self) -> bool {
                        let f = self;
                        let other = other;
                        { f.bits() & other.bits() != <u8 as ::bitflags::Bits>::EMPTY }
                    }
                    /// Whether all set bits in a source flags value are also set in a target flags value.
                    #[inline]
                    pub const fn contains(&self, other: Self) -> bool {
                        let f = self;
                        let other = other;
                        { f.bits() & other.bits() == other.bits() }
                    }
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    pub fn insert(&mut self, other: Self) {
                        let f = self;
                        let other = other;
                        {
                            *f = Self::from_bits_retain(f.bits()).union(other);
                        }
                    }
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `remove` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    pub fn remove(&mut self, other: Self) {
                        let f = self;
                        let other = other;
                        {
                            *f = Self::from_bits_retain(f.bits()).difference(other);
                        }
                    }
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    pub fn toggle(&mut self, other: Self) {
                        let f = self;
                        let other = other;
                        {
                            *f = Self::from_bits_retain(f.bits())
                                .symmetric_difference(other);
                        }
                    }
                    /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
                    #[inline]
                    pub fn set(&mut self, other: Self, value: bool) {
                        let f = self;
                        let other = other;
                        let value = value;
                        {
                            if value {
                                f.insert(other);
                            } else {
                                f.remove(other);
                            }
                        }
                    }
                    /// The bitwise and (`&`) of the bits in two flags values.
                    #[inline]
                    #[must_use]
                    pub const fn intersection(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self::from_bits_retain(f.bits() & other.bits()) }
                    }
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    #[must_use]
                    pub const fn union(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self::from_bits_retain(f.bits() | other.bits()) }
                    }
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `difference` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    #[must_use]
                    pub const fn difference(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self::from_bits_retain(f.bits() & !other.bits()) }
                    }
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    #[must_use]
                    pub const fn symmetric_difference(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self::from_bits_retain(f.bits() ^ other.bits()) }
                    }
                    /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                    #[inline]
                    #[must_use]
                    pub const fn complement(self) -> Self {
                        let f = self;
                        { Self::from_bits_truncate(!f.bits()) }
                    }
                }
                impl ::bitflags::__private::core::fmt::Binary for InternalBitFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::Binary::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::fmt::Octal for InternalBitFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::Octal::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::fmt::LowerHex for InternalBitFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::LowerHex::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::fmt::UpperHex for InternalBitFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::UpperHex::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::ops::BitOr for InternalBitFlags {
                    type Output = Self;
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    fn bitor(self, other: InternalBitFlags) -> Self {
                        self.union(other)
                    }
                }
                impl ::bitflags::__private::core::ops::BitOrAssign for InternalBitFlags {
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    fn bitor_assign(&mut self, other: Self) {
                        self.insert(other);
                    }
                }
                impl ::bitflags::__private::core::ops::BitXor for InternalBitFlags {
                    type Output = Self;
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    fn bitxor(self, other: Self) -> Self {
                        self.symmetric_difference(other)
                    }
                }
                impl ::bitflags::__private::core::ops::BitXorAssign
                for InternalBitFlags {
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    fn bitxor_assign(&mut self, other: Self) {
                        self.toggle(other);
                    }
                }
                impl ::bitflags::__private::core::ops::BitAnd for InternalBitFlags {
                    type Output = Self;
                    /// The bitwise and (`&`) of the bits in two flags values.
                    #[inline]
                    fn bitand(self, other: Self) -> Self {
                        self.intersection(other)
                    }
                }
                impl ::bitflags::__private::core::ops::BitAndAssign
                for InternalBitFlags {
                    /// The bitwise and (`&`) of the bits in two flags values.
                    #[inline]
                    fn bitand_assign(&mut self, other: Self) {
                        *self = Self::from_bits_retain(self.bits()).intersection(other);
                    }
                }
                impl ::bitflags::__private::core::ops::Sub for InternalBitFlags {
                    type Output = Self;
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `difference` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    fn sub(self, other: Self) -> Self {
                        self.difference(other)
                    }
                }
                impl ::bitflags::__private::core::ops::SubAssign for InternalBitFlags {
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `difference` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    fn sub_assign(&mut self, other: Self) {
                        self.remove(other);
                    }
                }
                impl ::bitflags::__private::core::ops::Not for InternalBitFlags {
                    type Output = Self;
                    /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                    #[inline]
                    fn not(self) -> Self {
                        self.complement()
                    }
                }
                impl ::bitflags::__private::core::iter::Extend<InternalBitFlags>
                for InternalBitFlags {
                    /// The bitwise or (`|`) of the bits in each flags value.
                    fn extend<
                        T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                    >(&mut self, iterator: T) {
                        for item in iterator {
                            self.insert(item)
                        }
                    }
                }
                impl ::bitflags::__private::core::iter::FromIterator<InternalBitFlags>
                for InternalBitFlags {
                    /// The bitwise or (`|`) of the bits in each flags value.
                    fn from_iter<
                        T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                    >(iterator: T) -> Self {
                        use ::bitflags::__private::core::iter::Extend;
                        let mut result = Self::empty();
                        result.extend(iterator);
                        result
                    }
                }
                impl InternalBitFlags {
                    /// Yield a set of contained flags values.
                    ///
                    /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
                    /// will be yielded together as a final flags value.
                    #[inline]
                    pub const fn iter(&self) -> ::bitflags::iter::Iter<LineStsFlags> {
                        ::bitflags::iter::Iter::__private_const_new(
                            <LineStsFlags as ::bitflags::Flags>::FLAGS,
                            LineStsFlags::from_bits_retain(self.bits()),
                            LineStsFlags::from_bits_retain(self.bits()),
                        )
                    }
                    /// Yield a set of contained named flags values.
                    ///
                    /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
                    /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
                    #[inline]
                    pub const fn iter_names(
                        &self,
                    ) -> ::bitflags::iter::IterNames<LineStsFlags> {
                        ::bitflags::iter::IterNames::__private_const_new(
                            <LineStsFlags as ::bitflags::Flags>::FLAGS,
                            LineStsFlags::from_bits_retain(self.bits()),
                            LineStsFlags::from_bits_retain(self.bits()),
                        )
                    }
                }
                impl ::bitflags::__private::core::iter::IntoIterator
                for InternalBitFlags {
                    type Item = LineStsFlags;
                    type IntoIter = ::bitflags::iter::Iter<LineStsFlags>;
                    fn into_iter(self) -> Self::IntoIter {
                        self.iter()
                    }
                }
                impl InternalBitFlags {
                    /// Returns a mutable reference to the raw value of the flags currently stored.
                    #[inline]
                    pub fn bits_mut(&mut self) -> &mut u8 {
                        &mut self.0
                    }
                }
                #[allow(dead_code, deprecated, unused_attributes)]
                impl LineStsFlags {
                    /// Get a flags value with all bits unset.
                    #[inline]
                    pub const fn empty() -> Self {
                        { Self(InternalBitFlags::empty()) }
                    }
                    /// Get a flags value with all known bits set.
                    #[inline]
                    pub const fn all() -> Self {
                        { Self(InternalBitFlags::all()) }
                    }
                    /// Get the underlying bits value.
                    ///
                    /// The returned value is exactly the bits set in this flags value.
                    #[inline]
                    pub const fn bits(&self) -> u8 {
                        let f = self;
                        { f.0.bits() }
                    }
                    /// Convert from a bits value.
                    ///
                    /// This method will return `None` if any unknown bits are set.
                    #[inline]
                    pub const fn from_bits(
                        bits: u8,
                    ) -> ::bitflags::__private::core::option::Option<Self> {
                        let bits = bits;
                        {
                            match InternalBitFlags::from_bits(bits) {
                                ::bitflags::__private::core::option::Option::Some(bits) => {
                                    ::bitflags::__private::core::option::Option::Some(
                                        Self(bits),
                                    )
                                }
                                ::bitflags::__private::core::option::Option::None => {
                                    ::bitflags::__private::core::option::Option::None
                                }
                            }
                        }
                    }
                    /// Convert from a bits value, unsetting any unknown bits.
                    #[inline]
                    pub const fn from_bits_truncate(bits: u8) -> Self {
                        let bits = bits;
                        { Self(InternalBitFlags::from_bits_truncate(bits)) }
                    }
                    /// Convert from a bits value exactly.
                    #[inline]
                    pub const fn from_bits_retain(bits: u8) -> Self {
                        let bits = bits;
                        { Self(InternalBitFlags::from_bits_retain(bits)) }
                    }
                    /// Get a flags value with the bits of a flag with the given name set.
                    ///
                    /// This method will return `None` if `name` is empty or doesn't
                    /// correspond to any named flag.
                    #[inline]
                    pub fn from_name(
                        name: &str,
                    ) -> ::bitflags::__private::core::option::Option<Self> {
                        let name = name;
                        {
                            match InternalBitFlags::from_name(name) {
                                ::bitflags::__private::core::option::Option::Some(bits) => {
                                    ::bitflags::__private::core::option::Option::Some(
                                        Self(bits),
                                    )
                                }
                                ::bitflags::__private::core::option::Option::None => {
                                    ::bitflags::__private::core::option::Option::None
                                }
                            }
                        }
                    }
                    /// Whether all bits in this flags value are unset.
                    #[inline]
                    pub const fn is_empty(&self) -> bool {
                        let f = self;
                        { f.0.is_empty() }
                    }
                    /// Whether all known bits in this flags value are set.
                    #[inline]
                    pub const fn is_all(&self) -> bool {
                        let f = self;
                        { f.0.is_all() }
                    }
                    /// Whether any set bits in a source flags value are also set in a target flags value.
                    #[inline]
                    pub const fn intersects(&self, other: Self) -> bool {
                        let f = self;
                        let other = other;
                        { f.0.intersects(other.0) }
                    }
                    /// Whether all set bits in a source flags value are also set in a target flags value.
                    #[inline]
                    pub const fn contains(&self, other: Self) -> bool {
                        let f = self;
                        let other = other;
                        { f.0.contains(other.0) }
                    }
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    pub fn insert(&mut self, other: Self) {
                        let f = self;
                        let other = other;
                        { f.0.insert(other.0) }
                    }
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `remove` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    pub fn remove(&mut self, other: Self) {
                        let f = self;
                        let other = other;
                        { f.0.remove(other.0) }
                    }
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    pub fn toggle(&mut self, other: Self) {
                        let f = self;
                        let other = other;
                        { f.0.toggle(other.0) }
                    }
                    /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
                    #[inline]
                    pub fn set(&mut self, other: Self, value: bool) {
                        let f = self;
                        let other = other;
                        let value = value;
                        { f.0.set(other.0, value) }
                    }
                    /// The bitwise and (`&`) of the bits in two flags values.
                    #[inline]
                    #[must_use]
                    pub const fn intersection(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self(f.0.intersection(other.0)) }
                    }
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    #[must_use]
                    pub const fn union(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self(f.0.union(other.0)) }
                    }
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `difference` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    #[must_use]
                    pub const fn difference(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self(f.0.difference(other.0)) }
                    }
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    #[must_use]
                    pub const fn symmetric_difference(self, other: Self) -> Self {
                        let f = self;
                        let other = other;
                        { Self(f.0.symmetric_difference(other.0)) }
                    }
                    /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                    #[inline]
                    #[must_use]
                    pub const fn complement(self) -> Self {
                        let f = self;
                        { Self(f.0.complement()) }
                    }
                }
                impl ::bitflags::__private::core::fmt::Binary for LineStsFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::Binary::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::fmt::Octal for LineStsFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::Octal::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::fmt::LowerHex for LineStsFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::LowerHex::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::fmt::UpperHex for LineStsFlags {
                    fn fmt(
                        &self,
                        f: &mut ::bitflags::__private::core::fmt::Formatter,
                    ) -> ::bitflags::__private::core::fmt::Result {
                        ::bitflags::__private::core::fmt::UpperHex::fmt(&self.0, f)
                    }
                }
                impl ::bitflags::__private::core::ops::BitOr for LineStsFlags {
                    type Output = Self;
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    fn bitor(self, other: LineStsFlags) -> Self {
                        self.union(other)
                    }
                }
                impl ::bitflags::__private::core::ops::BitOrAssign for LineStsFlags {
                    /// The bitwise or (`|`) of the bits in two flags values.
                    #[inline]
                    fn bitor_assign(&mut self, other: Self) {
                        self.insert(other);
                    }
                }
                impl ::bitflags::__private::core::ops::BitXor for LineStsFlags {
                    type Output = Self;
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    fn bitxor(self, other: Self) -> Self {
                        self.symmetric_difference(other)
                    }
                }
                impl ::bitflags::__private::core::ops::BitXorAssign for LineStsFlags {
                    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                    #[inline]
                    fn bitxor_assign(&mut self, other: Self) {
                        self.toggle(other);
                    }
                }
                impl ::bitflags::__private::core::ops::BitAnd for LineStsFlags {
                    type Output = Self;
                    /// The bitwise and (`&`) of the bits in two flags values.
                    #[inline]
                    fn bitand(self, other: Self) -> Self {
                        self.intersection(other)
                    }
                }
                impl ::bitflags::__private::core::ops::BitAndAssign for LineStsFlags {
                    /// The bitwise and (`&`) of the bits in two flags values.
                    #[inline]
                    fn bitand_assign(&mut self, other: Self) {
                        *self = Self::from_bits_retain(self.bits()).intersection(other);
                    }
                }
                impl ::bitflags::__private::core::ops::Sub for LineStsFlags {
                    type Output = Self;
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `difference` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    fn sub(self, other: Self) -> Self {
                        self.difference(other)
                    }
                }
                impl ::bitflags::__private::core::ops::SubAssign for LineStsFlags {
                    /// The intersection of a source flags value with the complement of a target flags value (`&!`).
                    ///
                    /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
                    /// `difference` won't truncate `other`, but the `!` operator will.
                    #[inline]
                    fn sub_assign(&mut self, other: Self) {
                        self.remove(other);
                    }
                }
                impl ::bitflags::__private::core::ops::Not for LineStsFlags {
                    type Output = Self;
                    /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
                    #[inline]
                    fn not(self) -> Self {
                        self.complement()
                    }
                }
                impl ::bitflags::__private::core::iter::Extend<LineStsFlags>
                for LineStsFlags {
                    /// The bitwise or (`|`) of the bits in each flags value.
                    fn extend<
                        T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                    >(&mut self, iterator: T) {
                        for item in iterator {
                            self.insert(item)
                        }
                    }
                }
                impl ::bitflags::__private::core::iter::FromIterator<LineStsFlags>
                for LineStsFlags {
                    /// The bitwise or (`|`) of the bits in each flags value.
                    fn from_iter<
                        T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
                    >(iterator: T) -> Self {
                        use ::bitflags::__private::core::iter::Extend;
                        let mut result = Self::empty();
                        result.extend(iterator);
                        result
                    }
                }
                impl LineStsFlags {
                    /// Yield a set of contained flags values.
                    ///
                    /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
                    /// will be yielded together as a final flags value.
                    #[inline]
                    pub const fn iter(&self) -> ::bitflags::iter::Iter<LineStsFlags> {
                        ::bitflags::iter::Iter::__private_const_new(
                            <LineStsFlags as ::bitflags::Flags>::FLAGS,
                            LineStsFlags::from_bits_retain(self.bits()),
                            LineStsFlags::from_bits_retain(self.bits()),
                        )
                    }
                    /// Yield a set of contained named flags values.
                    ///
                    /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
                    /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
                    #[inline]
                    pub const fn iter_names(
                        &self,
                    ) -> ::bitflags::iter::IterNames<LineStsFlags> {
                        ::bitflags::iter::IterNames::__private_const_new(
                            <LineStsFlags as ::bitflags::Flags>::FLAGS,
                            LineStsFlags::from_bits_retain(self.bits()),
                            LineStsFlags::from_bits_retain(self.bits()),
                        )
                    }
                }
                impl ::bitflags::__private::core::iter::IntoIterator for LineStsFlags {
                    type Item = LineStsFlags;
                    type IntoIter = ::bitflags::iter::Iter<LineStsFlags>;
                    fn into_iter(self) -> Self::IntoIter {
                        self.iter()
                    }
                }
            };
            struct Uart16550 {
                data: Port<u8>,
                int_en: PortWriteOnly<u8>,
                fifo_ctrl: PortWriteOnly<u8>,
                line_ctrl: PortWriteOnly<u8>,
                modem_ctrl: PortWriteOnly<u8>,
                line_sts: PortReadOnly<u8>,
            }
            impl Uart16550 {
                const fn new(port: u16) -> Self {
                    Self {
                        data: Port::new(port),
                        int_en: PortWriteOnly::new(port + 1),
                        fifo_ctrl: PortWriteOnly::new(port + 2),
                        line_ctrl: PortWriteOnly::new(port + 3),
                        modem_ctrl: PortWriteOnly::new(port + 4),
                        line_sts: PortReadOnly::new(port + 5),
                    }
                }
                fn init(&mut self, baud_rate: usize) {
                    unsafe {
                        self.int_en.write(0x00);
                        self.line_ctrl.write(0x80);
                        let divisor = OSC_FREQ / (baud_rate * UART_CLOCK_FACTOR);
                        self.data.write((divisor & 0xff) as u8);
                        self.int_en.write((divisor >> 8) as u8);
                        self.line_ctrl.write(0x03);
                        self.fifo_ctrl.write(0xC7);
                        self.modem_ctrl.write(0x0B);
                    }
                }
                fn line_sts(&mut self) -> LineStsFlags {
                    unsafe { LineStsFlags::from_bits_truncate(self.line_sts.read()) }
                }
                fn putchar(&mut self, c: u8) {
                    while !self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY) {}
                    unsafe { self.data.write(c) };
                }
                fn getchar(&mut self) -> Option<u8> {
                    if self.line_sts().contains(LineStsFlags::INPUT_FULL) {
                        unsafe { Some(self.data.read()) }
                    } else {
                        None
                    }
                }
            }
            /// Writes a byte to the console.
            pub fn putchar(c: u8) {
                let mut uart = COM1.lock();
                match c {
                    b'\n' => {
                        uart.putchar(b'\r');
                        uart.putchar(b'\n');
                    }
                    c => uart.putchar(c),
                }
            }
            /// Reads a byte from the console, or returns [`None`] if no input is available.
            pub fn getchar() -> Option<u8> {
                COM1.lock().getchar()
            }
            pub(super) fn init() {
                COM1.lock().init(115200);
            }
        }
        pub mod mem {
            use crate::mem::{
                virt_to_phys, MemRegion, MemRegionFlags, PhysAddr, VirtAddr,
            };
            use super::config::HvSystemConfig;
            use super::consts::{HV_BASE, PER_CPU_ARRAY_PTR};
            use super::header::HvHeader;
            extern "C" {
                fn __header_start();
                fn __header_end();
            }
            /// Returns the vmm free memory regions (kernel image end to physical memory end).
            fn vmm_free_regions() -> impl Iterator<Item = MemRegion> {
                let mem_pool_start = super::consts::free_memory_start();
                let mem_pool_end = super::consts::hv_end().align_down_4k();
                let mem_pool_size = mem_pool_end.as_usize() - mem_pool_start.as_usize();
                core::iter::once(MemRegion {
                    paddr: virt_to_phys(mem_pool_start),
                    size: mem_pool_size,
                    flags: MemRegionFlags::FREE | MemRegionFlags::READ
                        | MemRegionFlags::WRITE,
                    name: "free memory",
                })
            }
            fn vmm_per_cpu_data_regions() -> impl Iterator<Item = MemRegion> {
                let start_paddr = virt_to_phys(
                    VirtAddr::from(PER_CPU_ARRAY_PTR as usize),
                );
                let end_paddr = virt_to_phys(super::consts::free_memory_start());
                let size = end_paddr.as_usize() - start_paddr.as_usize();
                core::iter::once(MemRegion {
                    paddr: start_paddr,
                    size,
                    flags: MemRegionFlags::READ | MemRegionFlags::WRITE,
                    name: "per-CPU data, configurations",
                })
            }
            /// Returns platform-specific memory regions.
            pub(crate) fn platform_regions() -> impl Iterator<Item = MemRegion> {
                core::iter::once(MemRegion {
                        paddr: virt_to_phys((__header_start as usize).into()),
                        size: __header_end as usize - __header_start as usize,
                        flags: MemRegionFlags::RESERVED | MemRegionFlags::READ,
                        name: ".header",
                    })
                    .chain(vmm_per_cpu_data_regions())
                    .chain(vmm_free_regions())
                    .chain(crate::mem::default_mmio_regions())
            }
            pub fn host_memory_regions() -> impl Iterator<Item = MemRegion> {
                use crate::mem::MemRegionFlags;
                let sys_config = HvSystemConfig::get();
                let cell_config = &sys_config.root_cell.config();
                cell_config
                    .mem_regions()
                    .iter()
                    .filter(|region| {
                        MemRegionFlags::from_bits(region.flags)
                            .unwrap()
                            .contains(MemRegionFlags::DMA)
                    })
                    .map(|region| MemRegion {
                        paddr: PhysAddr::from(region.phys_start as usize),
                        size: region.size as usize,
                        flags: MemRegionFlags::from_bits(region.flags).unwrap(),
                        name: "Linux mem",
                    })
            }
        }
        pub mod misc {
            use x86_64::instructions::port::PortWriteOnly;
            /// Shutdown the whole system (in QEMU), including all CPUs.
            ///
            /// See <https://wiki.osdev.org/Shutdown> for more information.
            pub fn terminate() -> ! {
                {
                    let lvl = ::log::Level::Info;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!("Shutting down..."),
                            lvl,
                            &(
                                "axhal::platform::x86_linux::misc",
                                "axhal::platform::x86_linux::misc",
                                "modules/axhal/src/platform/x86_linux/misc.rs",
                                7u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                unsafe { PortWriteOnly::new(0x604).write(0x2000u16) };
                crate::arch::halt();
                {
                    let lvl = ::log::Level::Warn;
                    if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                        ::log::__private_api_log(
                            format_args!("It should shutdown!"),
                            lvl,
                            &(
                                "axhal::platform::x86_linux::misc",
                                "axhal::platform::x86_linux::misc",
                                "modules/axhal/src/platform/x86_linux/misc.rs",
                                15u32,
                            ),
                            ::log::__private_api::Option::None,
                        );
                    }
                };
                loop {
                    crate::arch::halt();
                }
            }
        }
        pub mod time {
            use raw_cpuid::CpuId;
            static mut INIT_TICK: u64 = 0;
            static mut CPU_FREQ_MHZ: u64 = axconfig::TIMER_FREQUENCY as u64 / 1_000_000;
            /// Returns the current clock time in hardware ticks.
            pub fn current_ticks() -> u64 {
                unsafe { core::arch::x86_64::_rdtsc() - INIT_TICK }
            }
            /// Converts hardware ticks to nanoseconds.
            pub fn ticks_to_nanos(ticks: u64) -> u64 {
                ticks * 1_000 / unsafe { CPU_FREQ_MHZ }
            }
            /// Converts nanoseconds to hardware ticks.
            pub fn nanos_to_ticks(nanos: u64) -> u64 {
                nanos * unsafe { CPU_FREQ_MHZ } / 1_000
            }
            pub(super) fn init_early() {
                if let Some(freq) = CpuId::new()
                    .get_processor_frequency_info()
                    .map(|info| info.processor_base_frequency())
                {
                    if freq > 0 {
                        ::axlog::__print_impl(
                            format_args!(
                                "{0}\n",
                                format_args!("Got TSC frequency by CPUID: {0} MHz", freq),
                            ),
                        );
                        unsafe { CPU_FREQ_MHZ = freq as u64 }
                    }
                }
                unsafe { INIT_TICK = core::arch::x86_64::_rdtsc() };
            }
            pub(super) fn init_primary() {}
        }
        mod percpu {
            use core::fmt::{Debug, Formatter, Result};
            use core::sync::atomic::{AtomicU32, Ordering};
            use super::consts::{PER_CPU_ARRAY_PTR, PER_CPU_SIZE};
            use super::current_cpu_id;
            use super::header::HvHeader;
            static ENTERED_CPUS: AtomicU32 = AtomicU32::new(0);
            static ACTIVATED_CPUS: AtomicU32 = AtomicU32::new(0);
            pub enum CpuState {
                HvDisabled,
                HvEnabled,
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for CpuState {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(
                        f,
                        match self {
                            CpuState::HvDisabled => "HvDisabled",
                            CpuState::HvEnabled => "HvEnabled",
                        },
                    )
                }
            }
            #[automatically_derived]
            impl ::core::marker::StructuralEq for CpuState {}
            #[automatically_derived]
            impl ::core::cmp::Eq for CpuState {
                #[inline]
                #[doc(hidden)]
                #[coverage(off)]
                fn assert_receiver_is_total_eq(&self) -> () {}
            }
            #[automatically_derived]
            impl ::core::marker::StructuralPartialEq for CpuState {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for CpuState {
                #[inline]
                fn eq(&self, other: &CpuState) -> bool {
                    let __self_tag = ::core::intrinsics::discriminant_value(self);
                    let __arg1_tag = ::core::intrinsics::discriminant_value(other);
                    __self_tag == __arg1_tag
                }
            }
            static mut BOOTED_CPU_APIC_ID: [u32; 255] = [u32::MAX; 255];
            #[repr(C, align(4096))]
            pub struct PerCpu {
                /// Referenced by arch::cpu::thread_pointer() for x86_64.
                self_vaddr: usize,
                /// Current CPU's apic id, read from raw_cpuid.
                pub id: u32,
                pub state: CpuState,
            }
            impl PerCpu {
                pub fn new<'a>() -> &'a mut Self {
                    if Self::entered_cpus() >= HvHeader::get().max_cpus {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "enter cpus exceed {0}",
                                    HvHeader::get().max_cpus,
                                ),
                            );
                        };
                    }
                    let cpu_sequence = ENTERED_CPUS.fetch_add(1, Ordering::SeqCst);
                    let cpu_id = current_cpu_id();
                    unsafe {
                        BOOTED_CPU_APIC_ID[cpu_id] = cpu_sequence;
                    }
                    let ret = unsafe { Self::from_id_mut(cpu_id as u32) };
                    let vaddr = ret as *const _ as usize;
                    ret.id = cpu_id as u32;
                    ret.self_vaddr = vaddr;
                    ret
                }
                pub unsafe fn from_id_mut<'a>(cpu_id: u32) -> &'a mut Self {
                    let vaddr = PER_CPU_ARRAY_PTR as usize
                        + cpu_id as usize * PER_CPU_SIZE;
                    &mut *(vaddr as *mut Self)
                }
                pub fn current<'a>() -> &'a Self {
                    Self::current_mut()
                }
                pub fn current_mut<'a>() -> &'a mut Self {
                    unsafe { &mut *(crate::arch::read_thread_pointer() as *mut Self) }
                }
                pub fn stack_top(&self) -> usize {
                    self as *const _ as usize + PER_CPU_SIZE - 8
                }
                pub fn entered_cpus() -> u32 {
                    ENTERED_CPUS.load(Ordering::Acquire)
                }
                pub fn activated_cpus() -> u32 {
                    ACTIVATED_CPUS.load(Ordering::Acquire)
                }
                pub fn cpu_is_booted(apic_id: usize) -> bool {
                    unsafe { BOOTED_CPU_APIC_ID[apic_id] != u32::MAX }
                }
            }
            impl Debug for PerCpu {
                fn fmt(&self, f: &mut Formatter) -> Result {
                    let mut res = f.debug_struct("PerCpu");
                    res.field("id", &self.id)
                        .field("self_vaddr", &self.self_vaddr)
                        .field("state", &self.state);
                    res.finish()
                }
            }
        }
        mod config {
            use core::fmt::{Debug, Formatter, Result};
            use core::{mem::size_of, slice};
            const CONFIG_SIGNATURE: [u8; 6] = *b"ARCEOS";
            const CONFIG_REVISION: u16 = 13;
            const HV_CELL_NAME_MAXLEN: usize = 31;
            /// The jailhouse cell configuration.
            ///
            /// @note Keep Config._HEADER_FORMAT in jailhouse-cell-linux in sync with this
            /// structure.
            #[repr(C, packed)]
            pub struct HvCellDesc {
                signature: [u8; 6],
                revision: u16,
                name: [u8; HV_CELL_NAME_MAXLEN + 1],
                id: u32,
                num_memory_regions: u32,
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for HvCellDesc {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field5_finish(
                        f,
                        "HvCellDesc",
                        "signature",
                        &{ self.signature },
                        "revision",
                        &{ self.revision },
                        "name",
                        &{ self.name },
                        "id",
                        &{ self.id },
                        "num_memory_regions",
                        &&{ self.num_memory_regions },
                    )
                }
            }
            #[repr(C, packed)]
            pub struct HvMemoryRegion {
                pub phys_start: u64,
                pub virt_start: u64,
                pub size: u64,
                pub flags: u64,
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for HvMemoryRegion {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field4_finish(
                        f,
                        "HvMemoryRegion",
                        "phys_start",
                        &{ self.phys_start },
                        "virt_start",
                        &{ self.virt_start },
                        "size",
                        &{ self.size },
                        "flags",
                        &&{ self.flags },
                    )
                }
            }
            /// General descriptor of the system.
            #[repr(C)]
            pub struct HvSystemConfig {
                pub signature: [u8; 6],
                pub revision: u16,
                /// RVM location in memory
                pub hypervisor_memory: HvMemoryRegion,
                /// RTOS location in memory
                pub rtos_memory: HvMemoryRegion,
                pub root_cell: HvCellDesc,
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for HvSystemConfig {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field5_finish(
                        f,
                        "HvSystemConfig",
                        "signature",
                        &self.signature,
                        "revision",
                        &self.revision,
                        "hypervisor_memory",
                        &self.hypervisor_memory,
                        "rtos_memory",
                        &self.rtos_memory,
                        "root_cell",
                        &&self.root_cell,
                    )
                }
            }
            pub struct CellConfig<'a> {
                desc: &'a HvCellDesc,
            }
            impl HvCellDesc {
                pub const fn config(&self) -> CellConfig {
                    CellConfig::from(self)
                }
                pub const fn config_size(&self) -> usize {
                    self.num_memory_regions as usize * size_of::<HvMemoryRegion>()
                }
            }
            impl HvSystemConfig {
                pub fn get<'a>() -> &'a Self {
                    unsafe { &*super::consts::hv_config_ptr() }
                }
                pub const fn size(&self) -> usize {
                    size_of::<Self>() + self.root_cell.config_size()
                }
                pub fn check(&self) {
                    match (&self.signature, &CONFIG_SIGNATURE) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(
                                    kind,
                                    &*left_val,
                                    &*right_val,
                                    ::core::option::Option::None,
                                );
                            }
                        }
                    };
                    match (&self.revision, &CONFIG_REVISION) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(
                                    kind,
                                    &*left_val,
                                    &*right_val,
                                    ::core::option::Option::None,
                                );
                            }
                        }
                    };
                }
            }
            impl<'a> CellConfig<'a> {
                const fn from(desc: &'a HvCellDesc) -> Self {
                    Self { desc }
                }
                fn config_ptr<T>(&self) -> *const T {
                    unsafe { (self.desc as *const HvCellDesc).add(1) as _ }
                }
                pub const fn size(&self) -> usize {
                    self.desc.config_size()
                }
                pub fn mem_regions(&self) -> &'static [HvMemoryRegion] {
                    unsafe {
                        let ptr = self.config_ptr() as _;
                        slice::from_raw_parts(ptr, self.desc.num_memory_regions as usize)
                    }
                }
            }
            impl Debug for CellConfig<'_> {
                fn fmt(&self, f: &mut Formatter) -> Result {
                    let name = self.desc.name;
                    let mut len = 0;
                    while name[len] != 0 {
                        len += 1;
                    }
                    f.debug_struct("CellConfig")
                        .field("name", &core::str::from_utf8(&name[..len]))
                        .field("size", &self.size())
                        .field("mem regions", &self.mem_regions())
                        .finish()
                }
            }
        }
        mod consts {
            use super::config::HvSystemConfig;
            use super::header::HvHeader;
            use super::percpu::PerCpu;
            use crate::mem::VirtAddr;
            /// Size of the hypervisor heap.
            pub const HV_HEAP_SIZE: usize = 32 * 1024 * 1024;
            /// Size of the per-CPU data (stack and other CPU-local data).
            pub const PER_CPU_SIZE: usize = 512 * 1024;
            /// Start virtual address of the hypervisor memory.
            pub const HV_BASE: usize = 0xffff_ff00_0000_0000;
            /// Pointer of the `HvHeader` structure.
            pub const HV_HEADER_PTR: *const HvHeader = __header_start as _;
            /// Pointer of the per-CPU data array.
            pub const PER_CPU_ARRAY_PTR: *mut PerCpu = _ekernel as _;
            /// Pointer of the `HvSystemConfig` structure.
            pub fn hv_config_ptr() -> *const HvSystemConfig {
                (PER_CPU_ARRAY_PTR as usize
                    + HvHeader::get().max_cpus as usize * PER_CPU_SIZE) as _
            }
            /// Pointer of the free memory pool.
            pub fn free_memory_start() -> VirtAddr {
                VirtAddr::from(hv_config_ptr() as usize + HvSystemConfig::get().size())
                    .align_up_4k()
            }
            /// End virtual address of the hypervisor memory.
            pub fn hv_end() -> VirtAddr {
                VirtAddr::from(
                    HV_BASE + HvSystemConfig::get().hypervisor_memory.size as usize,
                )
            }
            extern "C" {
                fn __header_start();
                fn _ekernel();
            }
        }
        mod header {
            use core::fmt::{Debug, Formatter, Result};
            use super::consts::{HV_HEADER_PTR, PER_CPU_SIZE};
            const HEADER_SIGNATURE: [u8; 8] = *b"ARCEOSIM";
            #[repr(C)]
            pub struct HvHeader {
                pub signature: [u8; 8],
                pub core_size: usize,
                pub percpu_size: usize,
                pub entry: usize,
                /// Available CPU numbers provided by current physical platform.
                pub max_cpus: u32,
                /// CPU numbers reserved for ArceOS.
                /// The Rest of Available CPUs will be reserved for host Linux.
                pub arceos_cpus: u32,
            }
            impl HvHeader {
                pub fn get<'a>() -> &'a Self {
                    unsafe { &*HV_HEADER_PTR }
                }
                pub fn reserved_cpus(&self) -> u32 {
                    if self.arceos_cpus < self.max_cpus {
                        self.max_cpus - self.arceos_cpus
                    } else {
                        {
                            let lvl = ::log::Level::Warn;
                            if lvl <= ::log::STATIC_MAX_LEVEL
                                && lvl <= ::log::max_level()
                            {
                                ::log::__private_api_log(
                                    format_args!(
                                        "Invalid HvHeader: arceos_cpus ({0}) >= max_cpus ({1})",
                                        self.arceos_cpus,
                                        self.max_cpus,
                                    ),
                                    lvl,
                                    &(
                                        "axhal::platform::x86_linux::header",
                                        "axhal::platform::x86_linux::header",
                                        "modules/axhal/src/platform/x86_linux/header.rs",
                                        29u32,
                                    ),
                                    ::log::__private_api::Option::None,
                                );
                            }
                        };
                        self.max_cpus
                    }
                }
            }
            #[repr(C)]
            struct HvHeaderStuff {
                signature: [u8; 8],
                core_size: unsafe extern "C" fn(),
                percpu_size: usize,
                entry: unsafe extern "C" fn(),
                max_cpus: u32,
                rt_cpus: u32,
            }
            extern "C" {
                fn __entry_offset();
                fn __kernel_size();
            }
            #[used]
            #[link_section = ".header"]
            static HEADER_STUFF: HvHeaderStuff = HvHeaderStuff {
                signature: HEADER_SIGNATURE,
                core_size: __kernel_size,
                percpu_size: PER_CPU_SIZE,
                entry: __entry_offset,
                max_cpus: 0,
                rt_cpus: 0,
            };
            impl Debug for HvHeader {
                fn fmt(&self, f: &mut Formatter) -> Result {
                    f.debug_struct("HvHeader")
                        .field("signature", &core::str::from_utf8(&self.signature))
                        .field("core_size", &self.core_size)
                        .field("percpu_size", &self.percpu_size)
                        .field("entry", &self.entry)
                        .field("max_cpus", &self.max_cpus)
                        .field("arceos_cpus", &self.arceos_cpus)
                        .field("reserved_cpus", &self.reserved_cpus())
                        .finish()
                }
            }
        }
        pub mod console {
            pub use super::uart16550::*;
        }
        pub use mem::host_memory_regions;
        use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
        use axlog::ax_println as println;
        use config::HvSystemConfig;
        use header::HvHeader;
        use percpu::PerCpu;
        static VMM_PRIMARY_INIT_OK: AtomicU32 = AtomicU32::new(0);
        static VMM_MAIN_INIT_OK: AtomicU32 = AtomicU32::new(0);
        static INIT_LATE_OK: AtomicU32 = AtomicU32::new(0);
        static ERROR_NUM: AtomicI32 = AtomicI32::new(0);
        fn has_err() -> bool {
            ERROR_NUM.load(Ordering::Acquire) != 0
        }
        fn wait_for(condition: impl Fn() -> bool) {
            while !has_err() && condition() {
                core::hint::spin_loop();
            }
            if has_err() {
                ::axlog::__print_impl(
                    format_args!("{0}\n", format_args!("[Error] Other cpu init failed!")),
                )
            }
        }
        fn wait_for_counter(counter: &AtomicU32, max_value: u32) {
            wait_for(|| counter.load(Ordering::Acquire) < max_value)
        }
        fn current_cpu_id() -> usize {
            match raw_cpuid::CpuId::new().get_feature_info() {
                Some(finfo) => finfo.initial_local_apic_id() as usize,
                None => 0,
            }
        }
        fn vmm_primary_init_early(cpu_id: usize) {
            crate::mem::clear_bss();
            crate::cpu::init_primary(cpu_id);
            ::axlog::__print_impl(
                format_args!("{0}\n", format_args!("HvHeader\n{0:#?}", HvHeader::get())),
            );
            let system_config = HvSystemConfig::get();
            ::axlog::__print_impl(
                format_args!(
                    "{0}\n",
                    format_args!(
                        "\nInitializing hypervisor on Core [{0}]...\nconfig_signature = {1:?}\nconfig_revision = {2}\n",
                        cpu_id,
                        core::str::from_utf8(&system_config.signature),
                        system_config.revision,
                    ),
                ),
            );
            VMM_PRIMARY_INIT_OK.store(1, Ordering::Release);
            unsafe {}
        }
        fn vmm_secondary_init_early(cpu_id: usize) {
            ::axlog::__print_impl(
                format_args!(
                    "{0}\n",
                    format_args!("ARCEOS CPU {0} secondary_init_early()", cpu_id),
                ),
            );
            unsafe {}
        }
        extern "sysv64" fn vmm_cpu_entry(
            cpu_data: &mut PerCpu,
            _linux_sp: usize,
            linux_tp: usize,
        ) -> i32 {
            let is_primary = cpu_data.id == 0;
            let vm_cpus = HvHeader::get().reserved_cpus();
            wait_for(|| PerCpu::entered_cpus() < vm_cpus);
            ::axlog::__print_impl(
                format_args!(
                    "{0}\n",
                    format_args!(
                        "{0} CPU {1} entered.",
                        if is_primary { "Primary" } else { "Secondary" },
                        cpu_data.id,
                    ),
                ),
            );
            if is_primary {
                vmm_primary_init_early(cpu_data.id as usize);
            } else {
                wait_for_counter(&VMM_PRIMARY_INIT_OK, 1);
                wait_for_counter(&VMM_MAIN_INIT_OK, 1);
                vmm_secondary_init_early(cpu_data.id as usize);
            }
            unsafe {
                x86::msr::wrmsr(x86::msr::IA32_GS_BASE, linux_tp as u64);
            }
            let code = 0;
            ::axlog::__print_impl(
                format_args!(
                    "{0}\n",
                    format_args!(
                        "{0} CPU {1} return back to driver with code {2}.",
                        if is_primary { "Primary" } else { "Secondary" },
                        cpu_data.id,
                        code,
                    ),
                ),
            );
            code
        }
        /// Initializes the platform devices for the primary CPU.
        pub fn platform_init() {}
    }
    pub use self::x86_linux::*;
}
pub mod arch {
    //! Architecture-specific types and operations.
    mod x86_64 {
        mod context {
            use core::{arch::asm, fmt};
            use memory_addr::VirtAddr;
            /// Saved registers when a trap (interrupt or exception) occurs.
            #[allow(missing_docs)]
            #[repr(C)]
            pub struct TrapFrame {
                pub rax: u64,
                pub rcx: u64,
                pub rdx: u64,
                pub rbx: u64,
                pub rbp: u64,
                pub rsi: u64,
                pub rdi: u64,
                pub r8: u64,
                pub r9: u64,
                pub r10: u64,
                pub r11: u64,
                pub r12: u64,
                pub r13: u64,
                pub r14: u64,
                pub r15: u64,
                pub vector: u64,
                pub error_code: u64,
                pub rip: u64,
                pub cs: u64,
                pub rflags: u64,
                pub rsp: u64,
                pub ss: u64,
            }
            #[automatically_derived]
            #[allow(missing_docs)]
            impl ::core::fmt::Debug for TrapFrame {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let names: &'static _ = &[
                        "rax",
                        "rcx",
                        "rdx",
                        "rbx",
                        "rbp",
                        "rsi",
                        "rdi",
                        "r8",
                        "r9",
                        "r10",
                        "r11",
                        "r12",
                        "r13",
                        "r14",
                        "r15",
                        "vector",
                        "error_code",
                        "rip",
                        "cs",
                        "rflags",
                        "rsp",
                        "ss",
                    ];
                    let values: &[&dyn ::core::fmt::Debug] = &[
                        &self.rax,
                        &self.rcx,
                        &self.rdx,
                        &self.rbx,
                        &self.rbp,
                        &self.rsi,
                        &self.rdi,
                        &self.r8,
                        &self.r9,
                        &self.r10,
                        &self.r11,
                        &self.r12,
                        &self.r13,
                        &self.r14,
                        &self.r15,
                        &self.vector,
                        &self.error_code,
                        &self.rip,
                        &self.cs,
                        &self.rflags,
                        &self.rsp,
                        &&self.ss,
                    ];
                    ::core::fmt::Formatter::debug_struct_fields_finish(
                        f,
                        "TrapFrame",
                        names,
                        values,
                    )
                }
            }
            #[automatically_derived]
            #[allow(missing_docs)]
            impl ::core::default::Default for TrapFrame {
                #[inline]
                fn default() -> TrapFrame {
                    TrapFrame {
                        rax: ::core::default::Default::default(),
                        rcx: ::core::default::Default::default(),
                        rdx: ::core::default::Default::default(),
                        rbx: ::core::default::Default::default(),
                        rbp: ::core::default::Default::default(),
                        rsi: ::core::default::Default::default(),
                        rdi: ::core::default::Default::default(),
                        r8: ::core::default::Default::default(),
                        r9: ::core::default::Default::default(),
                        r10: ::core::default::Default::default(),
                        r11: ::core::default::Default::default(),
                        r12: ::core::default::Default::default(),
                        r13: ::core::default::Default::default(),
                        r14: ::core::default::Default::default(),
                        r15: ::core::default::Default::default(),
                        vector: ::core::default::Default::default(),
                        error_code: ::core::default::Default::default(),
                        rip: ::core::default::Default::default(),
                        cs: ::core::default::Default::default(),
                        rflags: ::core::default::Default::default(),
                        rsp: ::core::default::Default::default(),
                        ss: ::core::default::Default::default(),
                    }
                }
            }
            #[automatically_derived]
            #[allow(missing_docs)]
            impl ::core::clone::Clone for TrapFrame {
                #[inline]
                fn clone(&self) -> TrapFrame {
                    TrapFrame {
                        rax: ::core::clone::Clone::clone(&self.rax),
                        rcx: ::core::clone::Clone::clone(&self.rcx),
                        rdx: ::core::clone::Clone::clone(&self.rdx),
                        rbx: ::core::clone::Clone::clone(&self.rbx),
                        rbp: ::core::clone::Clone::clone(&self.rbp),
                        rsi: ::core::clone::Clone::clone(&self.rsi),
                        rdi: ::core::clone::Clone::clone(&self.rdi),
                        r8: ::core::clone::Clone::clone(&self.r8),
                        r9: ::core::clone::Clone::clone(&self.r9),
                        r10: ::core::clone::Clone::clone(&self.r10),
                        r11: ::core::clone::Clone::clone(&self.r11),
                        r12: ::core::clone::Clone::clone(&self.r12),
                        r13: ::core::clone::Clone::clone(&self.r13),
                        r14: ::core::clone::Clone::clone(&self.r14),
                        r15: ::core::clone::Clone::clone(&self.r15),
                        vector: ::core::clone::Clone::clone(&self.vector),
                        error_code: ::core::clone::Clone::clone(&self.error_code),
                        rip: ::core::clone::Clone::clone(&self.rip),
                        cs: ::core::clone::Clone::clone(&self.cs),
                        rflags: ::core::clone::Clone::clone(&self.rflags),
                        rsp: ::core::clone::Clone::clone(&self.rsp),
                        ss: ::core::clone::Clone::clone(&self.ss),
                    }
                }
            }
            impl TrapFrame {
                /// Whether the trap is from userspace.
                pub const fn is_user(&self) -> bool {
                    self.cs & 0b11 == 3
                }
            }
            #[repr(C)]
            struct ContextSwitchFrame {
                r15: u64,
                r14: u64,
                r13: u64,
                r12: u64,
                rbx: u64,
                rbp: u64,
                rip: u64,
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for ContextSwitchFrame {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let names: &'static _ = &[
                        "r15",
                        "r14",
                        "r13",
                        "r12",
                        "rbx",
                        "rbp",
                        "rip",
                    ];
                    let values: &[&dyn ::core::fmt::Debug] = &[
                        &self.r15,
                        &self.r14,
                        &self.r13,
                        &self.r12,
                        &self.rbx,
                        &self.rbp,
                        &&self.rip,
                    ];
                    ::core::fmt::Formatter::debug_struct_fields_finish(
                        f,
                        "ContextSwitchFrame",
                        names,
                        values,
                    )
                }
            }
            #[automatically_derived]
            impl ::core::default::Default for ContextSwitchFrame {
                #[inline]
                fn default() -> ContextSwitchFrame {
                    ContextSwitchFrame {
                        r15: ::core::default::Default::default(),
                        r14: ::core::default::Default::default(),
                        r13: ::core::default::Default::default(),
                        r12: ::core::default::Default::default(),
                        rbx: ::core::default::Default::default(),
                        rbp: ::core::default::Default::default(),
                        rip: ::core::default::Default::default(),
                    }
                }
            }
            /// A 512-byte memory region for the FXSAVE/FXRSTOR instruction to save and
            /// restore the x87 FPU, MMX, XMM, and MXCSR registers.
            ///
            /// See <https://www.felixcloutier.com/x86/fxsave> for more details.
            #[allow(missing_docs)]
            #[repr(C, align(16))]
            pub struct FxsaveArea {
                pub fcw: u16,
                pub fsw: u16,
                pub ftw: u16,
                pub fop: u16,
                pub fip: u64,
                pub fdp: u64,
                pub mxcsr: u32,
                pub mxcsr_mask: u32,
                pub st: [u64; 16],
                pub xmm: [u64; 32],
                _padding: [u64; 12],
            }
            #[automatically_derived]
            #[allow(missing_docs)]
            impl ::core::fmt::Debug for FxsaveArea {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let names: &'static _ = &[
                        "fcw",
                        "fsw",
                        "ftw",
                        "fop",
                        "fip",
                        "fdp",
                        "mxcsr",
                        "mxcsr_mask",
                        "st",
                        "xmm",
                        "_padding",
                    ];
                    let values: &[&dyn ::core::fmt::Debug] = &[
                        &self.fcw,
                        &self.fsw,
                        &self.ftw,
                        &self.fop,
                        &self.fip,
                        &self.fdp,
                        &self.mxcsr,
                        &self.mxcsr_mask,
                        &self.st,
                        &self.xmm,
                        &&self._padding,
                    ];
                    ::core::fmt::Formatter::debug_struct_fields_finish(
                        f,
                        "FxsaveArea",
                        names,
                        values,
                    )
                }
            }
            #[allow(unknown_lints, eq_op)]
            const _: [(); 0
                - !{
                    const ASSERT: bool = core::mem::size_of::<FxsaveArea>() == 512;
                    ASSERT
                } as usize] = [];
            /// Extended state of a task, such as FP/SIMD states.
            pub struct ExtendedState {
                /// Memory region for the FXSAVE/FXRSTOR instruction.
                pub fxsave_area: FxsaveArea,
            }
            impl fmt::Debug for ExtendedState {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.debug_struct("ExtendedState")
                        .field("fxsave_area", &self.fxsave_area)
                        .finish()
                }
            }
            /// Saved hardware states of a task.
            ///
            /// The context usually includes:
            ///
            /// - Callee-saved registers
            /// - Stack pointer register
            /// - Thread pointer register (for thread-local storage, currently unsupported)
            /// - FP/SIMD registers
            ///
            /// On context switch, current task saves its context from CPU to memory,
            /// and the next task restores its context from memory to CPU.
            ///
            /// On x86_64, callee-saved registers are saved to the kernel stack by the
            /// `PUSH` instruction. So that [`rsp`] is the `RSP` after callee-saved
            /// registers are pushed, and [`kstack_top`] is the top of the kernel stack
            /// (`RSP` before any push).
            ///
            /// [`rsp`]: TaskContext::rsp
            /// [`kstack_top`]: TaskContext::kstack_top
            pub struct TaskContext {
                /// The kernel stack top of the task.
                pub kstack_top: VirtAddr,
                /// `RSP` after all callee-saved registers are pushed.
                pub rsp: u64,
                /// Thread Local Storage (TLS).
                pub fs_base: usize,
            }
            #[automatically_derived]
            impl ::core::fmt::Debug for TaskContext {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::debug_struct_field3_finish(
                        f,
                        "TaskContext",
                        "kstack_top",
                        &self.kstack_top,
                        "rsp",
                        &self.rsp,
                        "fs_base",
                        &&self.fs_base,
                    )
                }
            }
            impl TaskContext {
                /// Creates a new default context for a new task.
                pub const fn new() -> Self {
                    Self {
                        kstack_top: VirtAddr::from(0),
                        rsp: 0,
                        fs_base: 0,
                    }
                }
                /// Initializes the context for a new task, with the given entry point and
                /// kernel stack.
                pub fn init(
                    &mut self,
                    entry: usize,
                    kstack_top: VirtAddr,
                    tls_area: VirtAddr,
                ) {
                    unsafe {
                        let frame_ptr = (kstack_top.as_mut_ptr() as *mut u64).sub(1);
                        let frame_ptr = (frame_ptr as *mut ContextSwitchFrame).sub(1);
                        core::ptr::write(
                            frame_ptr,
                            ContextSwitchFrame {
                                rip: entry as _,
                                ..Default::default()
                            },
                        );
                        self.rsp = frame_ptr as u64;
                    }
                    self.kstack_top = kstack_top;
                    self.fs_base = tls_area.as_usize();
                }
                /// Switches to another task.
                ///
                /// It first saves the current task's context from CPU to this place, and then
                /// restores the next task's context from `next_ctx` to CPU.
                pub fn switch_to(&mut self, next_ctx: &Self) {
                    unsafe { context_switch(&mut self.rsp, &next_ctx.rsp) }
                }
            }
            #[naked]
            unsafe extern "C" fn context_switch(
                _current_stack: &mut u64,
                _next_stack: &u64,
            ) {
                asm!(
                    "\n        push    rbp\n        push    rbx\n        push    r12\n        push    r13\n        push    r14\n        push    r15\n        mov     [rdi], rsp\n\n        mov     rsp, [rsi]\n        pop     r15\n        pop     r14\n        pop     r13\n        pop     r12\n        pop     rbx\n        pop     rbp\n        ret",
                    options(noreturn)
                )
            }
        }
        mod gdt {
            use core::fmt;
            use x86_64::instructions::tables::{lgdt, load_tss};
            use x86_64::registers::segmentation::{Segment, SegmentSelector, CS};
            use x86_64::structures::gdt::{Descriptor, DescriptorFlags};
            use x86_64::structures::{tss::TaskStateSegment, DescriptorTablePointer};
            use x86_64::{addr::VirtAddr, PrivilegeLevel};
            /// A wrapper of the Global Descriptor Table (GDT) with maximum 16 entries.
            #[repr(align(16))]
            pub struct GdtStruct {
                table: [u64; 16],
            }
            impl GdtStruct {
                /// Kernel code segment for 32-bit mode.
                pub const KCODE32_SELECTOR: SegmentSelector = SegmentSelector::new(
                    1,
                    PrivilegeLevel::Ring0,
                );
                /// Kernel code segment for 64-bit mode.
                pub const KCODE64_SELECTOR: SegmentSelector = SegmentSelector::new(
                    2,
                    PrivilegeLevel::Ring0,
                );
                /// Kernel data segment.
                pub const KDATA_SELECTOR: SegmentSelector = SegmentSelector::new(
                    3,
                    PrivilegeLevel::Ring0,
                );
                /// User code segment for 32-bit mode.
                pub const UCODE32_SELECTOR: SegmentSelector = SegmentSelector::new(
                    4,
                    PrivilegeLevel::Ring3,
                );
                /// User data segment.
                pub const UDATA_SELECTOR: SegmentSelector = SegmentSelector::new(
                    5,
                    PrivilegeLevel::Ring3,
                );
                /// User code segment for 64-bit mode.
                pub const UCODE64_SELECTOR: SegmentSelector = SegmentSelector::new(
                    6,
                    PrivilegeLevel::Ring3,
                );
                /// TSS segment.
                pub const TSS_SELECTOR: SegmentSelector = SegmentSelector::new(
                    7,
                    PrivilegeLevel::Ring0,
                );
                /// Constructs a new GDT struct that filled with the default segment
                /// descriptors, including the given TSS segment.
                pub fn new(tss: &'static TaskStateSegment) -> Self {
                    let mut table = [0; 16];
                    table[1] = DescriptorFlags::KERNEL_CODE32.bits();
                    table[2] = DescriptorFlags::KERNEL_CODE64.bits();
                    table[3] = DescriptorFlags::KERNEL_DATA.bits();
                    table[4] = DescriptorFlags::USER_CODE32.bits();
                    table[5] = DescriptorFlags::USER_DATA.bits();
                    table[6] = DescriptorFlags::USER_CODE64.bits();
                    if let Descriptor::SystemSegment(low, high) = Descriptor::tss_segment(
                        tss,
                    ) {
                        table[7] = low;
                        table[8] = high;
                    }
                    Self { table }
                }
                /// Returns the GDT pointer (base and limit) that can be used in `lgdt`
                /// instruction.
                pub fn pointer(&self) -> DescriptorTablePointer {
                    DescriptorTablePointer {
                        base: VirtAddr::new(self.table.as_ptr() as u64),
                        limit: (core::mem::size_of_val(&self.table) - 1) as u16,
                    }
                }
                /// Loads the GDT into the CPU (executes the `lgdt` instruction), and
                /// updates the code segment register (`CS`).
                ///
                /// # Safety
                ///
                /// This function is unsafe because it manipulates the CPU's privileged
                /// states.
                pub unsafe fn load(&'static self) {
                    lgdt(&self.pointer());
                    CS::set_reg(Self::KCODE64_SELECTOR);
                }
                /// Loads the TSS into the CPU (executes the `ltr` instruction).
                ///
                /// # Safety
                ///
                /// This function is unsafe because it manipulates the CPU's privileged
                /// states.
                pub unsafe fn load_tss(&'static self) {
                    load_tss(Self::TSS_SELECTOR);
                }
            }
            impl fmt::Debug for GdtStruct {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.debug_struct("GdtStruct")
                        .field("pointer", &self.pointer())
                        .field("table", &self.table)
                        .finish()
                }
            }
        }
        mod idt {
            use core::fmt;
            use x86_64::addr::VirtAddr;
            use x86_64::structures::idt::{Entry, HandlerFunc, InterruptDescriptorTable};
            use x86_64::structures::DescriptorTablePointer;
            const NUM_INT: usize = 256;
            /// A wrapper of the Interrupt Descriptor Table (IDT).
            #[repr(transparent)]
            pub struct IdtStruct {
                table: InterruptDescriptorTable,
            }
            impl IdtStruct {
                /// Constructs a new IDT struct that filled with entries from
                /// `trap_handler_table`.
                #[allow(clippy::new_without_default)]
                pub fn new() -> Self {
                    extern "C" {
                        #[link_name = "trap_handler_table"]
                        static ENTRIES: [extern "C" fn(); NUM_INT];
                    }
                    let mut idt = Self {
                        table: InterruptDescriptorTable::new(),
                    };
                    let entries = unsafe {
                        core::slice::from_raw_parts_mut(
                            &mut idt.table as *mut _ as *mut Entry<HandlerFunc>,
                            NUM_INT,
                        )
                    };
                    for i in 0..NUM_INT {
                        entries[i]
                            .set_handler_fn(unsafe { core::mem::transmute(ENTRIES[i]) });
                    }
                    idt
                }
                /// Returns the IDT pointer (base and limit) that can be used in the `lidt`
                /// instruction.
                pub fn pointer(&self) -> DescriptorTablePointer {
                    DescriptorTablePointer {
                        base: VirtAddr::new(&self.table as *const _ as u64),
                        limit: (core::mem::size_of::<InterruptDescriptorTable>() - 1)
                            as u16,
                    }
                }
                /// Loads the IDT into the CPU (executes the `lidt` instruction).
                ///
                /// # Safety
                ///
                /// This function is unsafe because it manipulates the CPU's privileged
                /// states.
                pub unsafe fn load(&'static self) {
                    self.table.load();
                }
            }
            impl fmt::Debug for IdtStruct {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.debug_struct("IdtStruct")
                        .field("pointer", &self.pointer())
                        .field("table", &self.table)
                        .finish()
                }
            }
        }
        #[cfg(target_os = "none")]
        mod trap {
            use x86::{controlregs::cr2, irq::*};
            use super::context::TrapFrame;
            const IRQ_VECTOR_START: u8 = 0x20;
            const IRQ_VECTOR_END: u8 = 0xff;
            #[no_mangle]
            fn x86_trap_handler(tf: &TrapFrame) {
                match tf.vector as u8 {
                    PAGE_FAULT_VECTOR => {
                        if tf.is_user() {
                            {
                                let lvl = ::log::Level::Warn;
                                if lvl <= ::log::STATIC_MAX_LEVEL
                                    && lvl <= ::log::max_level()
                                {
                                    ::log::__private_api_log(
                                        format_args!(
                                            "User #PF @ {0:#x}, fault_vaddr={1:#x}, error_code={2:#x}",
                                            tf.rip,
                                            unsafe { cr2() },
                                            tf.error_code,
                                        ),
                                        lvl,
                                        &(
                                            "axhal::arch::x86_64::trap",
                                            "axhal::arch::x86_64::trap",
                                            "modules/axhal/src/arch/x86_64/trap.rs",
                                            15u32,
                                        ),
                                        ::log::__private_api::Option::None,
                                    );
                                }
                            };
                        } else {
                            {
                                ::core::panicking::panic_fmt(
                                    format_args!(
                                        "Kernel #PF @ {0:#x}, fault_vaddr={1:#x}, error_code={2:#x}:\n{3:#x?}",
                                        tf.rip,
                                        unsafe { cr2() },
                                        tf.error_code,
                                        tf,
                                    ),
                                );
                            };
                        }
                    }
                    BREAKPOINT_VECTOR => {
                        let lvl = ::log::Level::Debug;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api_log(
                                format_args!("#BP @ {0:#x} ", tf.rip),
                                lvl,
                                &(
                                    "axhal::arch::x86_64::trap",
                                    "axhal::arch::x86_64::trap",
                                    "modules/axhal/src/arch/x86_64/trap.rs",
                                    31u32,
                                ),
                                ::log::__private_api::Option::None,
                            );
                        }
                    }
                    GENERAL_PROTECTION_FAULT_VECTOR => {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "#GP @ {0:#x}, error_code={1:#x}:\n{2:#x?}",
                                    tf.rip,
                                    tf.error_code,
                                    tf,
                                ),
                            );
                        };
                    }
                    IRQ_VECTOR_START..=IRQ_VECTOR_END => {
                        crate::trap::handle_irq_extern(tf.vector as _)
                    }
                    _ => {
                        {
                            ::core::panicking::panic_fmt(
                                format_args!(
                                    "Unhandled exception {0} (error_code = {1:#x}) @ {2:#x}:\n{3:#x?}",
                                    tf.vector,
                                    tf.error_code,
                                    tf.rip,
                                    tf,
                                ),
                            );
                        };
                    }
                }
            }
        }
        use core::arch::asm;
        use memory_addr::{PhysAddr, VirtAddr};
        use x86::{controlregs, msr, tlb};
        use x86_64::instructions::interrupts;
        pub use self::context::{ExtendedState, FxsaveArea, TaskContext, TrapFrame};
        pub use self::gdt::GdtStruct;
        pub use self::idt::IdtStruct;
        pub use x86_64::structures::tss::TaskStateSegment;
        /// Allows the current CPU to respond to interrupts.
        #[inline]
        pub fn enable_irqs() {
            #[cfg(target_os = "none")] interrupts::enable()
        }
        /// Makes the current CPU to ignore interrupts.
        #[inline]
        pub fn disable_irqs() {
            #[cfg(target_os = "none")] interrupts::disable()
        }
        /// Returns whether the current CPU is allowed to respond to interrupts.
        #[inline]
        pub fn irqs_enabled() -> bool {
            interrupts::are_enabled()
        }
        /// Relaxes the current CPU and waits for interrupts.
        ///
        /// It must be called with interrupts enabled, otherwise it will never return.
        #[inline]
        pub fn wait_for_irqs() {
            if true { unsafe { asm!("hlt") } } else { core::hint::spin_loop() }
        }
        /// Halt the current CPU.
        #[inline]
        pub fn halt() {
            disable_irqs();
            wait_for_irqs();
        }
        /// Reads the register that stores the current page table root.
        ///
        /// Returns the physical address of the page table root.
        #[inline]
        pub fn read_page_table_root() -> PhysAddr {
            PhysAddr::from(unsafe { controlregs::cr3() } as usize).align_down_4k()
        }
        /// Writes the register to update the current page table root.
        ///
        /// # Safety
        ///
        /// This function is unsafe as it changes the virtual memory address space.
        pub unsafe fn write_page_table_root(root_paddr: PhysAddr) {
            let old_root = read_page_table_root();
            {
                let lvl = ::log::Level::Trace;
                if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                    ::log::__private_api_log(
                        format_args!(
                            "set page table root: {0:#x} => {1:#x}",
                            old_root,
                            root_paddr,
                        ),
                        lvl,
                        &(
                            "axhal::arch::x86_64",
                            "axhal::arch::x86_64",
                            "modules/axhal/src/arch/x86_64/mod.rs",
                            73u32,
                        ),
                        ::log::__private_api::Option::None,
                    );
                }
            };
            if old_root != root_paddr {
                controlregs::cr3_write(root_paddr.as_usize() as _)
            }
        }
        /// Flushes the TLB.
        ///
        /// If `vaddr` is [`None`], flushes the entire TLB. Otherwise, flushes the TLB
        /// entry that maps the given virtual address.
        #[inline]
        pub fn flush_tlb(vaddr: Option<VirtAddr>) {
            if let Some(vaddr) = vaddr {
                unsafe { tlb::flush(vaddr.into()) }
            } else {
                unsafe { tlb::flush_all() }
            }
        }
        /// Reads the thread pointer of the current CPU.
        ///
        /// It is used to implement TLS (Thread Local Storage).
        #[inline]
        pub fn read_thread_pointer() -> usize {
            unsafe { msr::rdmsr(msr::IA32_FS_BASE) as usize }
        }
        /// Writes the thread pointer of the current CPU.
        ///
        /// It is used to implement TLS (Thread Local Storage).
        ///
        /// # Safety
        ///
        /// This function is unsafe as it changes the CPU states.
        #[inline]
        pub unsafe fn write_thread_pointer(fs_base: usize) {
            unsafe { msr::wrmsr(msr::IA32_FS_BASE, fs_base as u64) }
        }
    }
    pub use self::x86_64::*;
}
pub mod cpu {
    //! CPU-related operations.
    #[link_section = ".percpu"]
    static mut __PERCPU_CPU_ID: usize = 0;
    ///Wrapper struct for the per-CPU data [`CPU_ID`]
    #[allow(non_camel_case_types)]
    struct CPU_ID_WRAPPER {}
    static CPU_ID: CPU_ID_WRAPPER = CPU_ID_WRAPPER {};
    impl CPU_ID_WRAPPER {
        /// Returns the offset relative to the per-CPU data area base on the current CPU.
        #[inline]
        pub fn offset(&self) -> usize {
            let value: usize;
            unsafe {
                asm!("movabs {0}, offset {1}", out(reg) value, sym __PERCPU_CPU_ID);
            }
            value
        }
        /// Returns the raw pointer of this per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn current_ptr(&self) -> *const usize {
            #[cfg(not(target_os = "macos"))]
            {
                let base: usize;
                #[cfg(target_arch = "x86_64")]
                {
                    asm!(
                        "mov {0}, gs:[offset __PERCPU_SELF_PTR]\nadd {0}, offset {1}",
                        out(reg) base, sym __PERCPU_CPU_ID
                    );
                    base as *const usize
                }
            }
        }
        /// Returns the reference of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn current_ref_raw(&self) -> &usize {
            &*self.current_ptr()
        }
        /// Returns the mutable reference of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        #[allow(clippy::mut_from_ref)]
        pub unsafe fn current_ref_mut_raw(&self) -> &mut usize {
            &mut *(self.current_ptr() as *mut usize)
        }
        /// Manipulate the per-CPU data on the current CPU in the given closure.
        /// Preemption will be disabled during the call.
        pub fn with_current<F, T>(&self, f: F) -> T
        where
            F: FnOnce(&mut usize) -> T,
        {
            f(unsafe { self.current_ref_mut_raw() })
        }
        /// Returns the value of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn read_current_raw(&self) -> usize {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_arch = "x86_64")]
                {
                    let value: usize;
                    asm!(
                        "mov {0:r}, qword ptr gs:[offset {1}]", out(reg) value, sym
                        __PERCPU_CPU_ID
                    );
                    value
                }
            }
        }
        /// Set the value of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn write_current_raw(&self, val: usize) {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_arch = "x86_64")]
                {
                    asm!(
                        "mov qword ptr gs:[offset {1}], {0:r}", in (reg) val as usize,
                        sym __PERCPU_CPU_ID
                    )
                }
            }
        }
        /// Returns the value of the per-CPU data on the current CPU. Preemption will
        /// be disabled during the call.
        pub fn read_current(&self) -> usize {
            unsafe { self.read_current_raw() }
        }
        /// Set the value of the per-CPU data on the current CPU. Preemption will
        /// be disabled during the call.
        pub fn write_current(&self, val: usize) {
            unsafe { self.write_current_raw(val) }
        }
    }
    #[link_section = ".percpu"]
    static mut __PERCPU_IS_BSP: bool = false;
    ///Wrapper struct for the per-CPU data [`IS_BSP`]
    #[allow(non_camel_case_types)]
    struct IS_BSP_WRAPPER {}
    static IS_BSP: IS_BSP_WRAPPER = IS_BSP_WRAPPER {};
    impl IS_BSP_WRAPPER {
        /// Returns the offset relative to the per-CPU data area base on the current CPU.
        #[inline]
        pub fn offset(&self) -> usize {
            let value: usize;
            unsafe {
                asm!("movabs {0}, offset {1}", out(reg) value, sym __PERCPU_IS_BSP);
            }
            value
        }
        /// Returns the raw pointer of this per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn current_ptr(&self) -> *const bool {
            #[cfg(not(target_os = "macos"))]
            {
                let base: usize;
                #[cfg(target_arch = "x86_64")]
                {
                    asm!(
                        "mov {0}, gs:[offset __PERCPU_SELF_PTR]\nadd {0}, offset {1}",
                        out(reg) base, sym __PERCPU_IS_BSP
                    );
                    base as *const bool
                }
            }
        }
        /// Returns the reference of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn current_ref_raw(&self) -> &bool {
            &*self.current_ptr()
        }
        /// Returns the mutable reference of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        #[allow(clippy::mut_from_ref)]
        pub unsafe fn current_ref_mut_raw(&self) -> &mut bool {
            &mut *(self.current_ptr() as *mut bool)
        }
        /// Manipulate the per-CPU data on the current CPU in the given closure.
        /// Preemption will be disabled during the call.
        pub fn with_current<F, T>(&self, f: F) -> T
        where
            F: FnOnce(&mut bool) -> T,
        {
            f(unsafe { self.current_ref_mut_raw() })
        }
        /// Returns the value of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn read_current_raw(&self) -> bool {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_arch = "x86_64")]
                {
                    let value: u8;
                    asm!(
                        "mov {0}, byte ptr gs:[offset {1}]", out(reg_byte) value, sym
                        __PERCPU_IS_BSP
                    );
                    value != 0
                }
            }
        }
        /// Set the value of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn write_current_raw(&self, val: bool) {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_arch = "x86_64")]
                {
                    asm!(
                        "mov byte ptr gs:[offset {1}], {0}", in (reg_byte) val as u8, sym
                        __PERCPU_IS_BSP
                    )
                }
            }
        }
        /// Returns the value of the per-CPU data on the current CPU. Preemption will
        /// be disabled during the call.
        pub fn read_current(&self) -> bool {
            unsafe { self.read_current_raw() }
        }
        /// Set the value of the per-CPU data on the current CPU. Preemption will
        /// be disabled during the call.
        pub fn write_current(&self, val: bool) {
            unsafe { self.write_current_raw(val) }
        }
    }
    #[link_section = ".percpu"]
    static mut __PERCPU_CURRENT_TASK_PTR: usize = 0;
    ///Wrapper struct for the per-CPU data [`CURRENT_TASK_PTR`]
    #[allow(non_camel_case_types)]
    struct CURRENT_TASK_PTR_WRAPPER {}
    static CURRENT_TASK_PTR: CURRENT_TASK_PTR_WRAPPER = CURRENT_TASK_PTR_WRAPPER {};
    impl CURRENT_TASK_PTR_WRAPPER {
        /// Returns the offset relative to the per-CPU data area base on the current CPU.
        #[inline]
        pub fn offset(&self) -> usize {
            let value: usize;
            unsafe {
                asm!(
                    "movabs {0}, offset {1}", out(reg) value, sym
                    __PERCPU_CURRENT_TASK_PTR
                );
            }
            value
        }
        /// Returns the raw pointer of this per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn current_ptr(&self) -> *const usize {
            #[cfg(not(target_os = "macos"))]
            {
                let base: usize;
                #[cfg(target_arch = "x86_64")]
                {
                    asm!(
                        "mov {0}, gs:[offset __PERCPU_SELF_PTR]\nadd {0}, offset {1}",
                        out(reg) base, sym __PERCPU_CURRENT_TASK_PTR
                    );
                    base as *const usize
                }
            }
        }
        /// Returns the reference of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn current_ref_raw(&self) -> &usize {
            &*self.current_ptr()
        }
        /// Returns the mutable reference of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        #[allow(clippy::mut_from_ref)]
        pub unsafe fn current_ref_mut_raw(&self) -> &mut usize {
            &mut *(self.current_ptr() as *mut usize)
        }
        /// Manipulate the per-CPU data on the current CPU in the given closure.
        /// Preemption will be disabled during the call.
        pub fn with_current<F, T>(&self, f: F) -> T
        where
            F: FnOnce(&mut usize) -> T,
        {
            f(unsafe { self.current_ref_mut_raw() })
        }
        /// Returns the value of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn read_current_raw(&self) -> usize {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_arch = "x86_64")]
                {
                    let value: usize;
                    asm!(
                        "mov {0:r}, qword ptr gs:[offset {1}]", out(reg) value, sym
                        __PERCPU_CURRENT_TASK_PTR
                    );
                    value
                }
            }
        }
        /// Set the value of the per-CPU data on the current CPU.
        ///
        /// # Safety
        ///
        /// Caller must ensure that preemption is disabled on the current CPU.
        #[inline]
        pub unsafe fn write_current_raw(&self, val: usize) {
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_arch = "x86_64")]
                {
                    asm!(
                        "mov qword ptr gs:[offset {1}], {0:r}", in (reg) val as usize,
                        sym __PERCPU_CURRENT_TASK_PTR
                    )
                }
            }
        }
        /// Returns the value of the per-CPU data on the current CPU. Preemption will
        /// be disabled during the call.
        pub fn read_current(&self) -> usize {
            unsafe { self.read_current_raw() }
        }
        /// Set the value of the per-CPU data on the current CPU. Preemption will
        /// be disabled during the call.
        pub fn write_current(&self, val: usize) {
            unsafe { self.write_current_raw(val) }
        }
    }
    /// Returns the ID of the current CPU.
    #[inline]
    pub fn this_cpu_id() -> usize {
        CPU_ID.read_current()
    }
    /// Returns whether the current CPU is the primary CPU (aka the bootstrap
    /// processor or BSP)
    #[inline]
    pub fn this_cpu_is_bsp() -> bool {
        IS_BSP.read_current()
    }
    /// Gets the pointer to the current task with preemption-safety.
    ///
    /// Preemption may be enabled when calling this function. This function will
    /// guarantee the correctness even the current task is preempted.
    #[inline]
    pub fn current_task_ptr<T>() -> *const T {
        #[cfg(target_arch = "x86_64")]
        unsafe { CURRENT_TASK_PTR.read_current_raw() as _ }
    }
    /// Sets the pointer to the current task with preemption-safety.
    ///
    /// Preemption may be enabled when calling this function. This function will
    /// guarantee the correctness even the current task is preempted.
    ///
    /// # Safety
    ///
    /// The given `ptr` must be pointed to a valid task structure.
    #[inline]
    pub unsafe fn set_current_task_ptr<T>(ptr: *const T) {
        #[cfg(target_arch = "x86_64")]
        { CURRENT_TASK_PTR.write_current_raw(ptr as usize) }
    }
    #[allow(dead_code)]
    pub(crate) fn init_primary(cpu_id: usize) {
        percpu::init(axconfig::SMP);
        percpu::set_local_thread_pointer(cpu_id);
        unsafe {
            CPU_ID.write_current_raw(cpu_id);
            IS_BSP.write_current_raw(true);
        }
    }
    #[allow(dead_code)]
    pub(crate) fn init_secondary(cpu_id: usize) {
        percpu::set_local_thread_pointer(cpu_id);
        unsafe {
            CPU_ID.write_current_raw(cpu_id);
            IS_BSP.write_current_raw(false);
        }
    }
}
pub mod mem {
    //! Physical memory management.
    use core::fmt;
    #[doc(no_inline)]
    pub use memory_addr::{PhysAddr, VirtAddr, PAGE_SIZE_4K};
    /// The flags of a physical memory region.
    pub struct MemRegionFlags(
        <MemRegionFlags as ::bitflags::__private::PublicFlags>::Internal,
    );
    impl MemRegionFlags {
        /// Readable.
        #[allow(deprecated, non_upper_case_globals)]
        pub const READ: Self = Self::from_bits_retain(1 << 0);
        /// Writable.
        #[allow(deprecated, non_upper_case_globals)]
        pub const WRITE: Self = Self::from_bits_retain(1 << 1);
        /// Executable.
        #[allow(deprecated, non_upper_case_globals)]
        pub const EXECUTE: Self = Self::from_bits_retain(1 << 2);
        /// DMA.
        #[allow(deprecated, non_upper_case_globals)]
        pub const DMA: Self = Self::from_bits_retain(1 << 3);
        /// Device memory. (e.g., MMIO regions)
        #[allow(deprecated, non_upper_case_globals)]
        pub const DEVICE: Self = Self::from_bits_retain(1 << 4);
        /// Uncachable memory. (e.g., framebuffer)
        #[allow(deprecated, non_upper_case_globals)]
        pub const UNCACHED: Self = Self::from_bits_retain(1 << 5);
        /// Reserved memory, do not use for allocation.
        #[allow(deprecated, non_upper_case_globals)]
        pub const RESERVED: Self = Self::from_bits_retain(1 << 6);
        /// Free memory for allocation.
        #[allow(deprecated, non_upper_case_globals)]
        pub const FREE: Self = Self::from_bits_retain(1 << 7);
    }
    impl ::bitflags::Flags for MemRegionFlags {
        const FLAGS: &'static [::bitflags::Flag<MemRegionFlags>] = &[
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("READ", MemRegionFlags::READ)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("WRITE", MemRegionFlags::WRITE)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("EXECUTE", MemRegionFlags::EXECUTE)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("DMA", MemRegionFlags::DMA)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("DEVICE", MemRegionFlags::DEVICE)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("UNCACHED", MemRegionFlags::UNCACHED)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("RESERVED", MemRegionFlags::RESERVED)
            },
            {
                #[allow(deprecated, non_upper_case_globals)]
                ::bitflags::Flag::new("FREE", MemRegionFlags::FREE)
            },
        ];
        type Bits = u64;
        fn bits(&self) -> u64 {
            MemRegionFlags::bits(self)
        }
        fn from_bits_retain(bits: u64) -> MemRegionFlags {
            MemRegionFlags::from_bits_retain(bits)
        }
    }
    #[allow(
        dead_code,
        deprecated,
        unused_doc_comments,
        unused_attributes,
        unused_mut,
        unused_imports,
        non_upper_case_globals,
        clippy::assign_op_pattern,
        clippy::indexing_slicing,
        clippy::same_name_method
    )]
    const _: () = {
        #[repr(transparent)]
        pub struct InternalBitFlags(u64);
        #[automatically_derived]
        impl ::core::clone::Clone for InternalBitFlags {
            #[inline]
            fn clone(&self) -> InternalBitFlags {
                let _: ::core::clone::AssertParamIsClone<u64>;
                *self
            }
        }
        #[automatically_derived]
        impl ::core::marker::Copy for InternalBitFlags {}
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for InternalBitFlags {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for InternalBitFlags {
            #[inline]
            fn eq(&self, other: &InternalBitFlags) -> bool {
                self.0 == other.0
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralEq for InternalBitFlags {}
        #[automatically_derived]
        impl ::core::cmp::Eq for InternalBitFlags {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {
                let _: ::core::cmp::AssertParamIsEq<u64>;
            }
        }
        #[automatically_derived]
        impl ::core::cmp::PartialOrd for InternalBitFlags {
            #[inline]
            fn partial_cmp(
                &self,
                other: &InternalBitFlags,
            ) -> ::core::option::Option<::core::cmp::Ordering> {
                ::core::cmp::PartialOrd::partial_cmp(&self.0, &other.0)
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Ord for InternalBitFlags {
            #[inline]
            fn cmp(&self, other: &InternalBitFlags) -> ::core::cmp::Ordering {
                ::core::cmp::Ord::cmp(&self.0, &other.0)
            }
        }
        #[automatically_derived]
        impl ::core::hash::Hash for InternalBitFlags {
            #[inline]
            fn hash<__H: ::core::hash::Hasher>(&self, state: &mut __H) -> () {
                ::core::hash::Hash::hash(&self.0, state)
            }
        }
        impl ::bitflags::__private::PublicFlags for MemRegionFlags {
            type Primitive = u64;
            type Internal = InternalBitFlags;
        }
        impl ::bitflags::__private::core::default::Default for InternalBitFlags {
            #[inline]
            fn default() -> Self {
                InternalBitFlags::empty()
            }
        }
        impl ::bitflags::__private::core::fmt::Debug for InternalBitFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter<'_>,
            ) -> ::bitflags::__private::core::fmt::Result {
                if self.is_empty() {
                    f.write_fmt(format_args!("{0:#x}", <u64 as ::bitflags::Bits>::EMPTY))
                } else {
                    ::bitflags::__private::core::fmt::Display::fmt(self, f)
                }
            }
        }
        impl ::bitflags::__private::core::fmt::Display for InternalBitFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter<'_>,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::parser::to_writer(&MemRegionFlags(*self), f)
            }
        }
        impl ::bitflags::__private::core::str::FromStr for InternalBitFlags {
            type Err = ::bitflags::parser::ParseError;
            fn from_str(
                s: &str,
            ) -> ::bitflags::__private::core::result::Result<Self, Self::Err> {
                ::bitflags::parser::from_str::<MemRegionFlags>(s).map(|flags| flags.0)
            }
        }
        impl ::bitflags::__private::core::convert::AsRef<u64> for InternalBitFlags {
            fn as_ref(&self) -> &u64 {
                &self.0
            }
        }
        impl ::bitflags::__private::core::convert::From<u64> for InternalBitFlags {
            fn from(bits: u64) -> Self {
                Self::from_bits_retain(bits)
            }
        }
        #[allow(dead_code, deprecated, unused_attributes)]
        impl InternalBitFlags {
            /// Get a flags value with all bits unset.
            #[inline]
            pub const fn empty() -> Self {
                { Self(<u64 as ::bitflags::Bits>::EMPTY) }
            }
            /// Get a flags value with all known bits set.
            #[inline]
            pub const fn all() -> Self {
                {
                    let mut truncated = <u64 as ::bitflags::Bits>::EMPTY;
                    let mut i = 0;
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    {
                        {
                            let flag = <MemRegionFlags as ::bitflags::Flags>::FLAGS[i]
                                .value()
                                .bits();
                            truncated = truncated | flag;
                            i += 1;
                        }
                    };
                    let _ = i;
                    Self::from_bits_retain(truncated)
                }
            }
            /// Get the underlying bits value.
            ///
            /// The returned value is exactly the bits set in this flags value.
            #[inline]
            pub const fn bits(&self) -> u64 {
                let f = self;
                { f.0 }
            }
            /// Convert from a bits value.
            ///
            /// This method will return `None` if any unknown bits are set.
            #[inline]
            pub const fn from_bits(
                bits: u64,
            ) -> ::bitflags::__private::core::option::Option<Self> {
                let bits = bits;
                {
                    let truncated = Self::from_bits_truncate(bits).0;
                    if truncated == bits {
                        ::bitflags::__private::core::option::Option::Some(Self(bits))
                    } else {
                        ::bitflags::__private::core::option::Option::None
                    }
                }
            }
            /// Convert from a bits value, unsetting any unknown bits.
            #[inline]
            pub const fn from_bits_truncate(bits: u64) -> Self {
                let bits = bits;
                { Self(bits & Self::all().bits()) }
            }
            /// Convert from a bits value exactly.
            #[inline]
            pub const fn from_bits_retain(bits: u64) -> Self {
                let bits = bits;
                { Self(bits) }
            }
            /// Get a flags value with the bits of a flag with the given name set.
            ///
            /// This method will return `None` if `name` is empty or doesn't
            /// correspond to any named flag.
            #[inline]
            pub fn from_name(
                name: &str,
            ) -> ::bitflags::__private::core::option::Option<Self> {
                let name = name;
                {
                    {
                        if name == "READ" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::READ.bits()),
                            );
                        }
                    };
                    {
                        if name == "WRITE" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::WRITE.bits()),
                            );
                        }
                    };
                    {
                        if name == "EXECUTE" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::EXECUTE.bits()),
                            );
                        }
                    };
                    {
                        if name == "DMA" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::DMA.bits()),
                            );
                        }
                    };
                    {
                        if name == "DEVICE" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::DEVICE.bits()),
                            );
                        }
                    };
                    {
                        if name == "UNCACHED" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::UNCACHED.bits()),
                            );
                        }
                    };
                    {
                        if name == "RESERVED" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::RESERVED.bits()),
                            );
                        }
                    };
                    {
                        if name == "FREE" {
                            return ::bitflags::__private::core::option::Option::Some(
                                Self(MemRegionFlags::FREE.bits()),
                            );
                        }
                    };
                    let _ = name;
                    ::bitflags::__private::core::option::Option::None
                }
            }
            /// Whether all bits in this flags value are unset.
            #[inline]
            pub const fn is_empty(&self) -> bool {
                let f = self;
                { f.bits() == <u64 as ::bitflags::Bits>::EMPTY }
            }
            /// Whether all known bits in this flags value are set.
            #[inline]
            pub const fn is_all(&self) -> bool {
                let f = self;
                { Self::all().bits() | f.bits() == f.bits() }
            }
            /// Whether any set bits in a source flags value are also set in a target flags value.
            #[inline]
            pub const fn intersects(&self, other: Self) -> bool {
                let f = self;
                let other = other;
                { f.bits() & other.bits() != <u64 as ::bitflags::Bits>::EMPTY }
            }
            /// Whether all set bits in a source flags value are also set in a target flags value.
            #[inline]
            pub const fn contains(&self, other: Self) -> bool {
                let f = self;
                let other = other;
                { f.bits() & other.bits() == other.bits() }
            }
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            pub fn insert(&mut self, other: Self) {
                let f = self;
                let other = other;
                {
                    *f = Self::from_bits_retain(f.bits()).union(other);
                }
            }
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `remove` won't truncate `other`, but the `!` operator will.
            #[inline]
            pub fn remove(&mut self, other: Self) {
                let f = self;
                let other = other;
                {
                    *f = Self::from_bits_retain(f.bits()).difference(other);
                }
            }
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            pub fn toggle(&mut self, other: Self) {
                let f = self;
                let other = other;
                {
                    *f = Self::from_bits_retain(f.bits()).symmetric_difference(other);
                }
            }
            /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
            #[inline]
            pub fn set(&mut self, other: Self, value: bool) {
                let f = self;
                let other = other;
                let value = value;
                {
                    if value {
                        f.insert(other);
                    } else {
                        f.remove(other);
                    }
                }
            }
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn intersection(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self::from_bits_retain(f.bits() & other.bits()) }
            }
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn union(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self::from_bits_retain(f.bits() | other.bits()) }
            }
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            #[must_use]
            pub const fn difference(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self::from_bits_retain(f.bits() & !other.bits()) }
            }
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn symmetric_difference(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self::from_bits_retain(f.bits() ^ other.bits()) }
            }
            /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
            #[inline]
            #[must_use]
            pub const fn complement(self) -> Self {
                let f = self;
                { Self::from_bits_truncate(!f.bits()) }
            }
        }
        impl ::bitflags::__private::core::fmt::Binary for InternalBitFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::Binary::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::fmt::Octal for InternalBitFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::Octal::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::fmt::LowerHex for InternalBitFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::LowerHex::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::fmt::UpperHex for InternalBitFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::UpperHex::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::ops::BitOr for InternalBitFlags {
            type Output = Self;
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor(self, other: InternalBitFlags) -> Self {
                self.union(other)
            }
        }
        impl ::bitflags::__private::core::ops::BitOrAssign for InternalBitFlags {
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.insert(other);
            }
        }
        impl ::bitflags::__private::core::ops::BitXor for InternalBitFlags {
            type Output = Self;
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor(self, other: Self) -> Self {
                self.symmetric_difference(other)
            }
        }
        impl ::bitflags::__private::core::ops::BitXorAssign for InternalBitFlags {
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor_assign(&mut self, other: Self) {
                self.toggle(other);
            }
        }
        impl ::bitflags::__private::core::ops::BitAnd for InternalBitFlags {
            type Output = Self;
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand(self, other: Self) -> Self {
                self.intersection(other)
            }
        }
        impl ::bitflags::__private::core::ops::BitAndAssign for InternalBitFlags {
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                *self = Self::from_bits_retain(self.bits()).intersection(other);
            }
        }
        impl ::bitflags::__private::core::ops::Sub for InternalBitFlags {
            type Output = Self;
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            fn sub(self, other: Self) -> Self {
                self.difference(other)
            }
        }
        impl ::bitflags::__private::core::ops::SubAssign for InternalBitFlags {
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            fn sub_assign(&mut self, other: Self) {
                self.remove(other);
            }
        }
        impl ::bitflags::__private::core::ops::Not for InternalBitFlags {
            type Output = Self;
            /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
            #[inline]
            fn not(self) -> Self {
                self.complement()
            }
        }
        impl ::bitflags::__private::core::iter::Extend<InternalBitFlags>
        for InternalBitFlags {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn extend<T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>>(
                &mut self,
                iterator: T,
            ) {
                for item in iterator {
                    self.insert(item)
                }
            }
        }
        impl ::bitflags::__private::core::iter::FromIterator<InternalBitFlags>
        for InternalBitFlags {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn from_iter<
                T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
            >(iterator: T) -> Self {
                use ::bitflags::__private::core::iter::Extend;
                let mut result = Self::empty();
                result.extend(iterator);
                result
            }
        }
        impl InternalBitFlags {
            /// Yield a set of contained flags values.
            ///
            /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
            /// will be yielded together as a final flags value.
            #[inline]
            pub const fn iter(&self) -> ::bitflags::iter::Iter<MemRegionFlags> {
                ::bitflags::iter::Iter::__private_const_new(
                    <MemRegionFlags as ::bitflags::Flags>::FLAGS,
                    MemRegionFlags::from_bits_retain(self.bits()),
                    MemRegionFlags::from_bits_retain(self.bits()),
                )
            }
            /// Yield a set of contained named flags values.
            ///
            /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
            /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
            #[inline]
            pub const fn iter_names(
                &self,
            ) -> ::bitflags::iter::IterNames<MemRegionFlags> {
                ::bitflags::iter::IterNames::__private_const_new(
                    <MemRegionFlags as ::bitflags::Flags>::FLAGS,
                    MemRegionFlags::from_bits_retain(self.bits()),
                    MemRegionFlags::from_bits_retain(self.bits()),
                )
            }
        }
        impl ::bitflags::__private::core::iter::IntoIterator for InternalBitFlags {
            type Item = MemRegionFlags;
            type IntoIter = ::bitflags::iter::Iter<MemRegionFlags>;
            fn into_iter(self) -> Self::IntoIter {
                self.iter()
            }
        }
        impl InternalBitFlags {
            /// Returns a mutable reference to the raw value of the flags currently stored.
            #[inline]
            pub fn bits_mut(&mut self) -> &mut u64 {
                &mut self.0
            }
        }
        #[allow(dead_code, deprecated, unused_attributes)]
        impl MemRegionFlags {
            /// Get a flags value with all bits unset.
            #[inline]
            pub const fn empty() -> Self {
                { Self(InternalBitFlags::empty()) }
            }
            /// Get a flags value with all known bits set.
            #[inline]
            pub const fn all() -> Self {
                { Self(InternalBitFlags::all()) }
            }
            /// Get the underlying bits value.
            ///
            /// The returned value is exactly the bits set in this flags value.
            #[inline]
            pub const fn bits(&self) -> u64 {
                let f = self;
                { f.0.bits() }
            }
            /// Convert from a bits value.
            ///
            /// This method will return `None` if any unknown bits are set.
            #[inline]
            pub const fn from_bits(
                bits: u64,
            ) -> ::bitflags::__private::core::option::Option<Self> {
                let bits = bits;
                {
                    match InternalBitFlags::from_bits(bits) {
                        ::bitflags::__private::core::option::Option::Some(bits) => {
                            ::bitflags::__private::core::option::Option::Some(Self(bits))
                        }
                        ::bitflags::__private::core::option::Option::None => {
                            ::bitflags::__private::core::option::Option::None
                        }
                    }
                }
            }
            /// Convert from a bits value, unsetting any unknown bits.
            #[inline]
            pub const fn from_bits_truncate(bits: u64) -> Self {
                let bits = bits;
                { Self(InternalBitFlags::from_bits_truncate(bits)) }
            }
            /// Convert from a bits value exactly.
            #[inline]
            pub const fn from_bits_retain(bits: u64) -> Self {
                let bits = bits;
                { Self(InternalBitFlags::from_bits_retain(bits)) }
            }
            /// Get a flags value with the bits of a flag with the given name set.
            ///
            /// This method will return `None` if `name` is empty or doesn't
            /// correspond to any named flag.
            #[inline]
            pub fn from_name(
                name: &str,
            ) -> ::bitflags::__private::core::option::Option<Self> {
                let name = name;
                {
                    match InternalBitFlags::from_name(name) {
                        ::bitflags::__private::core::option::Option::Some(bits) => {
                            ::bitflags::__private::core::option::Option::Some(Self(bits))
                        }
                        ::bitflags::__private::core::option::Option::None => {
                            ::bitflags::__private::core::option::Option::None
                        }
                    }
                }
            }
            /// Whether all bits in this flags value are unset.
            #[inline]
            pub const fn is_empty(&self) -> bool {
                let f = self;
                { f.0.is_empty() }
            }
            /// Whether all known bits in this flags value are set.
            #[inline]
            pub const fn is_all(&self) -> bool {
                let f = self;
                { f.0.is_all() }
            }
            /// Whether any set bits in a source flags value are also set in a target flags value.
            #[inline]
            pub const fn intersects(&self, other: Self) -> bool {
                let f = self;
                let other = other;
                { f.0.intersects(other.0) }
            }
            /// Whether all set bits in a source flags value are also set in a target flags value.
            #[inline]
            pub const fn contains(&self, other: Self) -> bool {
                let f = self;
                let other = other;
                { f.0.contains(other.0) }
            }
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            pub fn insert(&mut self, other: Self) {
                let f = self;
                let other = other;
                { f.0.insert(other.0) }
            }
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `remove` won't truncate `other`, but the `!` operator will.
            #[inline]
            pub fn remove(&mut self, other: Self) {
                let f = self;
                let other = other;
                { f.0.remove(other.0) }
            }
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            pub fn toggle(&mut self, other: Self) {
                let f = self;
                let other = other;
                { f.0.toggle(other.0) }
            }
            /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
            #[inline]
            pub fn set(&mut self, other: Self, value: bool) {
                let f = self;
                let other = other;
                let value = value;
                { f.0.set(other.0, value) }
            }
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn intersection(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self(f.0.intersection(other.0)) }
            }
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn union(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self(f.0.union(other.0)) }
            }
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            #[must_use]
            pub const fn difference(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self(f.0.difference(other.0)) }
            }
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn symmetric_difference(self, other: Self) -> Self {
                let f = self;
                let other = other;
                { Self(f.0.symmetric_difference(other.0)) }
            }
            /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
            #[inline]
            #[must_use]
            pub const fn complement(self) -> Self {
                let f = self;
                { Self(f.0.complement()) }
            }
        }
        impl ::bitflags::__private::core::fmt::Binary for MemRegionFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::Binary::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::fmt::Octal for MemRegionFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::Octal::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::fmt::LowerHex for MemRegionFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::LowerHex::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::fmt::UpperHex for MemRegionFlags {
            fn fmt(
                &self,
                f: &mut ::bitflags::__private::core::fmt::Formatter,
            ) -> ::bitflags::__private::core::fmt::Result {
                ::bitflags::__private::core::fmt::UpperHex::fmt(&self.0, f)
            }
        }
        impl ::bitflags::__private::core::ops::BitOr for MemRegionFlags {
            type Output = Self;
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor(self, other: MemRegionFlags) -> Self {
                self.union(other)
            }
        }
        impl ::bitflags::__private::core::ops::BitOrAssign for MemRegionFlags {
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.insert(other);
            }
        }
        impl ::bitflags::__private::core::ops::BitXor for MemRegionFlags {
            type Output = Self;
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor(self, other: Self) -> Self {
                self.symmetric_difference(other)
            }
        }
        impl ::bitflags::__private::core::ops::BitXorAssign for MemRegionFlags {
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor_assign(&mut self, other: Self) {
                self.toggle(other);
            }
        }
        impl ::bitflags::__private::core::ops::BitAnd for MemRegionFlags {
            type Output = Self;
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand(self, other: Self) -> Self {
                self.intersection(other)
            }
        }
        impl ::bitflags::__private::core::ops::BitAndAssign for MemRegionFlags {
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                *self = Self::from_bits_retain(self.bits()).intersection(other);
            }
        }
        impl ::bitflags::__private::core::ops::Sub for MemRegionFlags {
            type Output = Self;
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            fn sub(self, other: Self) -> Self {
                self.difference(other)
            }
        }
        impl ::bitflags::__private::core::ops::SubAssign for MemRegionFlags {
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            fn sub_assign(&mut self, other: Self) {
                self.remove(other);
            }
        }
        impl ::bitflags::__private::core::ops::Not for MemRegionFlags {
            type Output = Self;
            /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
            #[inline]
            fn not(self) -> Self {
                self.complement()
            }
        }
        impl ::bitflags::__private::core::iter::Extend<MemRegionFlags>
        for MemRegionFlags {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn extend<T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>>(
                &mut self,
                iterator: T,
            ) {
                for item in iterator {
                    self.insert(item)
                }
            }
        }
        impl ::bitflags::__private::core::iter::FromIterator<MemRegionFlags>
        for MemRegionFlags {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn from_iter<
                T: ::bitflags::__private::core::iter::IntoIterator<Item = Self>,
            >(iterator: T) -> Self {
                use ::bitflags::__private::core::iter::Extend;
                let mut result = Self::empty();
                result.extend(iterator);
                result
            }
        }
        impl MemRegionFlags {
            /// Yield a set of contained flags values.
            ///
            /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
            /// will be yielded together as a final flags value.
            #[inline]
            pub const fn iter(&self) -> ::bitflags::iter::Iter<MemRegionFlags> {
                ::bitflags::iter::Iter::__private_const_new(
                    <MemRegionFlags as ::bitflags::Flags>::FLAGS,
                    MemRegionFlags::from_bits_retain(self.bits()),
                    MemRegionFlags::from_bits_retain(self.bits()),
                )
            }
            /// Yield a set of contained named flags values.
            ///
            /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
            /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
            #[inline]
            pub const fn iter_names(
                &self,
            ) -> ::bitflags::iter::IterNames<MemRegionFlags> {
                ::bitflags::iter::IterNames::__private_const_new(
                    <MemRegionFlags as ::bitflags::Flags>::FLAGS,
                    MemRegionFlags::from_bits_retain(self.bits()),
                    MemRegionFlags::from_bits_retain(self.bits()),
                )
            }
        }
        impl ::bitflags::__private::core::iter::IntoIterator for MemRegionFlags {
            type Item = MemRegionFlags;
            type IntoIter = ::bitflags::iter::Iter<MemRegionFlags>;
            fn into_iter(self) -> Self::IntoIter {
                self.iter()
            }
        }
    };
    impl fmt::Debug for MemRegionFlags {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Debug::fmt(&self.0, f)
        }
    }
    /// A physical memory region.
    pub struct MemRegion {
        /// The start physical address of the region.
        pub paddr: PhysAddr,
        /// The size in bytes of the region.
        pub size: usize,
        /// The region flags, see [`MemRegionFlags`].
        pub flags: MemRegionFlags,
        /// The region name, used for identification.
        pub name: &'static str,
    }
    #[automatically_derived]
    impl ::core::fmt::Debug for MemRegion {
        #[inline]
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            ::core::fmt::Formatter::debug_struct_field4_finish(
                f,
                "MemRegion",
                "paddr",
                &self.paddr,
                "size",
                &self.size,
                "flags",
                &self.flags,
                "name",
                &&self.name,
            )
        }
    }
    /// Converts a virtual address to a physical address.
    ///
    /// It assumes that there is a linear mapping with the offset
    /// [`PHYS_VIRT_OFFSET`], that maps all the physical memory to the virtual
    /// space at the address plus the offset. So we have
    /// `paddr = vaddr - PHYS_VIRT_OFFSET`.
    ///
    /// [`PHYS_VIRT_OFFSET`]: axconfig::PHYS_VIRT_OFFSET
    #[inline]
    pub const fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
        PhysAddr::from(vaddr.as_usize() - axconfig::PHYS_VIRT_OFFSET)
    }
    /// Converts a physical address to a virtual address.
    ///
    /// It assumes that there is a linear mapping with the offset
    /// [`PHYS_VIRT_OFFSET`], that maps all the physical memory to the virtual
    /// space at the address plus the offset. So we have
    /// `vaddr = paddr + PHYS_VIRT_OFFSET`.
    ///
    /// [`PHYS_VIRT_OFFSET`]: axconfig::PHYS_VIRT_OFFSET
    #[inline]
    pub const fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        VirtAddr::from(paddr.as_usize() + axconfig::PHYS_VIRT_OFFSET)
    }
    /// Returns an iterator over all physical memory regions.
    pub fn memory_regions() -> impl Iterator<Item = MemRegion> {
        kernel_image_regions().chain(crate::platform::mem::platform_regions())
    }
    /// Returns the memory regions of the kernel image (code and data sections).
    fn kernel_image_regions() -> impl Iterator<Item = MemRegion> {
        [
            MemRegion {
                paddr: virt_to_phys((_stext as usize).into()),
                size: _etext as usize - _stext as usize,
                flags: MemRegionFlags::RESERVED | MemRegionFlags::READ
                    | MemRegionFlags::EXECUTE,
                name: ".text",
            },
            MemRegion {
                paddr: virt_to_phys((_srodata as usize).into()),
                size: _erodata as usize - _srodata as usize,
                flags: MemRegionFlags::RESERVED | MemRegionFlags::READ,
                name: ".rodata",
            },
            MemRegion {
                paddr: virt_to_phys((_sdata as usize).into()),
                size: _edata as usize - _sdata as usize,
                flags: MemRegionFlags::RESERVED | MemRegionFlags::READ
                    | MemRegionFlags::WRITE,
                name: ".data .tdata .tbss .percpu",
            },
            MemRegion {
                paddr: virt_to_phys((boot_stack as usize).into()),
                size: boot_stack_top as usize - boot_stack as usize,
                flags: MemRegionFlags::RESERVED | MemRegionFlags::READ
                    | MemRegionFlags::WRITE,
                name: "boot stack",
            },
            MemRegion {
                paddr: virt_to_phys((_sbss as usize).into()),
                size: _ebss as usize - _sbss as usize,
                flags: MemRegionFlags::RESERVED | MemRegionFlags::READ
                    | MemRegionFlags::WRITE,
                name: ".bss",
            },
        ]
            .into_iter()
    }
    /// Returns the default MMIO memory regions (from [`axconfig::MMIO_REGIONS`]).
    #[allow(dead_code)]
    pub(crate) fn default_mmio_regions() -> impl Iterator<Item = MemRegion> {
        axconfig::MMIO_REGIONS
            .iter()
            .map(|reg| MemRegion {
                paddr: reg.0.into(),
                size: reg.1,
                flags: MemRegionFlags::RESERVED | MemRegionFlags::DEVICE
                    | MemRegionFlags::READ | MemRegionFlags::WRITE,
                name: "mmio",
            })
    }
    /// Returns the default free memory regions (kernel image end to physical memory end).
    #[allow(dead_code)]
    pub(crate) fn default_free_regions() -> impl Iterator<Item = MemRegion> {
        let start = virt_to_phys((_ekernel as usize).into()).align_up_4k();
        let end = PhysAddr::from(axconfig::PHYS_MEMORY_END).align_down_4k();
        core::iter::once(MemRegion {
            paddr: start,
            size: end.as_usize() - start.as_usize(),
            flags: MemRegionFlags::FREE | MemRegionFlags::READ | MemRegionFlags::WRITE,
            name: "free memory",
        })
    }
    /// Fills the `.bss` section with zeros.
    #[allow(dead_code)]
    pub(crate) fn clear_bss() {
        unsafe {
            core::slice::from_raw_parts_mut(
                    _sbss as usize as *mut u8,
                    _ebss as usize - _sbss as usize,
                )
                .fill(0);
        }
    }
    extern "C" {
        fn _stext();
        fn _etext();
        fn _srodata();
        fn _erodata();
        fn _sdata();
        fn _edata();
        fn _sbss();
        fn _ebss();
        fn _ekernel();
        fn boot_stack();
        fn boot_stack_top();
    }
}
pub mod time {
    //! Time-related operations.
    pub use core::time::Duration;
    /// A measurement of the system clock.
    ///
    /// Currently, it reuses the [`core::time::Duration`] type. But it does not
    /// represent a duration, but a clock time.
    pub type TimeValue = Duration;
    pub use crate::platform::time::{current_ticks, nanos_to_ticks, ticks_to_nanos};
    /// Number of milliseconds in a second.
    pub const MILLIS_PER_SEC: u64 = 1_000;
    /// Number of microseconds in a second.
    pub const MICROS_PER_SEC: u64 = 1_000_000;
    /// Number of nanoseconds in a second.
    pub const NANOS_PER_SEC: u64 = 1_000_000_000;
    /// Number of nanoseconds in a millisecond.
    pub const NANOS_PER_MILLIS: u64 = 1_000_000;
    /// Number of nanoseconds in a microsecond.
    pub const NANOS_PER_MICROS: u64 = 1_000;
    /// Returns the current clock time in nanoseconds.
    pub fn current_time_nanos() -> u64 {
        ticks_to_nanos(current_ticks())
    }
    /// Returns the current clock time in [`TimeValue`].
    pub fn current_time() -> TimeValue {
        TimeValue::from_nanos(current_time_nanos())
    }
    /// Busy waiting for the given duration.
    pub fn busy_wait(dur: Duration) {
        busy_wait_until(current_time() + dur);
    }
    /// Busy waiting until reaching the given deadline.
    pub fn busy_wait_until(deadline: TimeValue) {
        while current_time() < deadline {
            core::hint::spin_loop();
        }
    }
}
pub mod trap {
    //! Trap handling.
    use crate_interface::{call_interface, def_interface};
    /// Trap handler interface.
    ///
    /// This trait is defined with the [`#[def_interface]`][1] attribute. Users
    /// should implement it with [`#[impl_interface]`][2] in any other crate.
    ///
    /// [1]: crate_interface::def_interface
    /// [2]: crate_interface::impl_interface
    pub trait TrapHandler {
        /// Handles interrupt requests for the given IRQ number.
        fn handle_irq(irq_num: usize);
    }
    extern "Rust" {
        fn __TrapHandler_handle_irq(irq_num: usize);
    }
    /// Call the external IRQ handler.
    #[allow(dead_code)]
    pub(crate) fn handle_irq_extern(irq_num: usize) {
        unsafe { __TrapHandler_handle_irq(irq_num) };
    }
}
/// Console input and output.
pub mod console {
    pub use super::platform::console::*;
    /// Write a slice of bytes to the console.
    pub fn write_bytes(bytes: &[u8]) {
        for c in bytes {
            putchar(*c);
        }
    }
}
/// Miscellaneous operation, e.g. terminate the system.
pub mod misc {
    pub use super::platform::misc::*;
}
pub use self::platform::platform_init;
pub use self::platform::mem::host_memory_regions;
