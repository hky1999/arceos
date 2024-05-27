#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[cfg(feature = "axstd")]
extern crate axstd as std;

#[macro_use]
extern crate log;

mod hal;

use axvm::AxvmPerCpu;

use self::hal::AxvmHalImpl;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    info!("Starting virtualization...");
    info!("Hardware support: {:?}", axvm::has_hardware_support());

    let mut percpu = AxvmPerCpu::<AxvmHalImpl>::new(0);
    percpu
        .hardware_enable()
        .expect("Failed to enable virtualization");

    let mut vcpu = percpu.create_vcpu().unwrap();
    info!("{:#x?}", vcpu);
    vcpu.run();
}
