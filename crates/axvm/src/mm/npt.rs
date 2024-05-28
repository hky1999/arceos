use axhal::paging::PagingIfImpl;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        /// The architecture-specific nested page table for two-stage address translation.
        pub type NestedPageTable = crate::arch::X64NestedPageTable<PagingIfImpl>;
    } else if #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))] {
        /// The architecture-specific page table.
        // pub type NestedPageTable = page_table::riscv::Sv39PageTable<PagingIfImpl>;
    } else if #[cfg(target_arch = "aarch64")]{
        /// The architecture-specific page table.
        // pub type NestedPageTable = page_table::aarch64::A64PageTable<PagingIfImpl>;
    }
}
