use axhal::hv::HyperCraftHalImpl;

struct VMExitHandlerImpl;

#[crate_interface::impl_interface]
impl axhal::hv::VMExitHandler for VMExitHandlerImpl {
    fn vmexit_handler(vcpu: &mut hypercraft::VCpu<HyperCraftHalImpl>) -> hypercraft::HyperResult {
        let exit_info = vcpu.exit_info()?;
        trace!("VM exit: {:#x?}", exit_info);

        Ok(())
    }
}
