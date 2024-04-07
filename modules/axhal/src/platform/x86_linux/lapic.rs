use spin::{Once, RwLock};
use x86::apic::{x2apic::X2APIC, xapic::XAPIC, ApicControl, ApicId};

extern crate alloc;

use alloc::sync::Arc;

use crate::mem::{phys_to_virt, PhysAddr};

use self::vectors::*;

pub(super) mod vectors {
    pub const APIC_TIMER_VECTOR: u8 = 0xf0;
    pub const APIC_SPURIOUS_VECTOR: u8 = 0xf1;
    pub const APIC_ERROR_VECTOR: u8 = 0xf2;
}

/// The maximum number of IRQs.
pub const MAX_IRQ_COUNT: usize = 256;

/// The timer IRQ number.
pub const TIMER_IRQ_NUM: usize = APIC_TIMER_VECTOR as usize;

const APIC_BASE: PhysAddr = PhysAddr::from(0xFEE0_0000);
const MAX_APIC_ID: u32 = 254;

bitflags::bitflags! {
    /// IA32_APIC_BASE MSR.
    struct ApicBase: u64 {
        /// Processor is BSP.
        const BSP   = 1 << 8;
        /// Enable x2APIC mode.
        const EXTD  = 1 << 10;
        /// xAPIC global enable/disable.
        const EN    = 1 << 11;
    }
}

impl ApicBase {
    pub fn read() -> Self {
        unsafe { Self::from_bits_retain(x86::msr::rdmsr(x86::msr::IA32_APIC_BASE)) }
    }
}

pub(super) struct LocalApic {
    inner: Arc<RwLock<dyn ApicControl>>,
    is_x2apic: bool,
}

unsafe impl Send for LocalApic {}
unsafe impl Sync for LocalApic {}

impl LocalApic {
    pub fn new() -> Result<Self, &'static str> {
        let base = ApicBase::read();

        debug!("ApicBase {:#x}", base.bits());

        if base.contains(ApicBase::EXTD) {
            info!("Using x2APIC.");
            Ok(Self {
                inner: Arc::new(RwLock::new(X2APIC::new())),
                is_x2apic: true,
            })
        } else if base.contains(ApicBase::EN) {
            info!("Using xAPIC.");
            let base_vaddr = phys_to_virt(APIC_BASE);
            let apic_region =
                unsafe { core::slice::from_raw_parts_mut(base_vaddr.as_usize() as _, 0x1000 / 4) };
            Ok(Self {
                inner: Arc::new(RwLock::new(XAPIC::new(apic_region))),
                is_x2apic: false,
            })
        } else {
            Err("Local Apic init failed")
        }
    }

    pub fn id(&self) -> u32 {
        if self.is_x2apic {
            self.inner.read().id()
        } else {
            self.inner.read().id() >> 24
        }
    }
}

static LOCAL_APIC: Once<LocalApic> = Once::new();
static mut APIC_TO_CPU_ID: [u32; MAX_APIC_ID as usize + 1] = [u32::MAX; MAX_APIC_ID as usize + 1];

pub(super) fn lapic<'a>() -> &'a LocalApic {
    LOCAL_APIC.get().expect("Uninitialized Local APIC!")
}

pub(super) fn apic_to_cpu_id(apic_id: u32) -> u32 {
    if apic_id <= MAX_APIC_ID {
        unsafe { APIC_TO_CPU_ID[apic_id as usize] }
    } else {
        u32::MAX
    }
}

pub(super) fn init() {
    let lapic = LocalApic::new().expect("LocalApic init failed");
    LOCAL_APIC.call_once(|| lapic);
}

use crate::time::{busy_wait, Duration};

pub(super) unsafe fn start_ap(apic_id: u32, start_page_idx: u8) {
    info!("Starting RT cpu {}...", apic_id);
    let apic_id = if lapic().is_x2apic {
        ApicId::X2Apic(apic_id)
    } else {
        ApicId::XApic(apic_id as u8)
    };

    // INIT-SIPI-SIPI Sequence
    let mut lapic = lapic().inner.write();
    lapic.ipi_init(apic_id);
    // delay_us(10 * 1000); // 10ms
    busy_wait(Duration::from_millis(10)); // 10ms
    lapic.ipi_startup(apic_id, start_page_idx);
    // delay_us(200); // 200 us
    busy_wait(Duration::from_micros(200)); // 200us
    lapic.ipi_startup(apic_id, start_page_idx);
}

pub(super) unsafe fn shutdown_ap(apic_id: u32) {
    info!("Shutting down RT cpu {}...", apic_id);
    let apic_id = if lapic().is_x2apic {
        ApicId::X2Apic(apic_id)
    } else {
        ApicId::XApic(apic_id as u8)
    };

    lapic().inner.write().ipi_init(apic_id);
}

/// Enables or disables the given IRQ.
#[cfg(feature = "irq")]
pub fn set_enable(vector: usize, enabled: bool) {
    // // should not affect LAPIC interrupts
    // if vector < APIC_TIMER_VECTOR as _ {
    //     unsafe {
    //         if enabled {
    //             IO_APIC.lock().enable_irq(vector as u8);
    //         } else {
    //             IO_APIC.lock().disable_irq(vector as u8);
    //         }
    //     }
    // }
}

/// Registers an IRQ handler for the given IRQ.
///
/// It also enables the IRQ if the registration succeeds. It returns `false` if
/// the registration failed.
#[cfg(feature = "irq")]
pub fn register_handler(vector: usize, handler: crate::irq::IrqHandler) -> bool {
    crate::irq::register_handler_common(vector, handler)
}

/// Dispatches the IRQ.
///
/// This function is called by the common interrupt handler. It looks
/// up in the IRQ handler table and calls the corresponding handler. If
/// necessary, it also acknowledges the interrupt controller after handling.
#[cfg(feature = "irq")]
pub fn dispatch_irq(vector: usize) {
    // crate::irq::dispatch_irq_common(vector);
    // unsafe { local_apic().end_of_interrupt() };
}
