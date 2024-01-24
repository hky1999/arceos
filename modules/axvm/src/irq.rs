use crate::Result;

use axhal::irq::{dispatch_irq, set_enable};


pub(crate) fn dispatch_host_irq(vector: usize) -> Result {
    dispatch_irq(vector);
    Ok(())
}


pub(crate) fn set_host_irq_enabled(vector: usize, enabled: bool) -> Result {
    set_enable(vector, enabled);
    Ok(())
}