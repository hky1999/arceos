/// Bundle for CMOS, NMI, PIT and Speaker

extern crate alloc;
use bit_field::BitField;
use crate::{Result as HyperResult, Error as HyperError};
use super::pit::PIT;
use super::{pmio_proxy_factory, pmio_proxy_struct};

pub const PORT_SYSTEM_CONTROL_A: u16 = 0x92;
pub const PORT_SYSTEM_CONTROL_B: u16 = 0x61;

pub const PORT_CMOS_ADDRESS: u16 = 0x70;
pub const PORT_CMOS_DATA: u16 = 0x71;

pub const PORT_PIT_CHANNEL_DATA_BASE: u16 = 0x40;
pub const PORT_PIT_COMMAND: u16 = 0x43;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct SystemControlPortB: u8 {
        const TIMER2_ENABLED = 1 << 0;
        const SPEAKER_ENABLED = 1 << 1;
        const PARITY_CHECK_ENABLED = 1 << 2;
        const CHANNEL_CHECK_ENABLED = 1 << 3;

        const TIMER1_OUTPUT = 1 << 4;
        const TIMER2_OUTPUT = 1 << 5;
        const CHANNEL_CHECK = 1 << 6;
        const PARITY_CHECK = 1 << 7;

        const WRITABLE_MASK = 0b0000_1111;
        const READONLY_MASK = 0b1111_0000;
    }
}


pub struct Bundle {
    // about cmos
    cmos_selected_reg: Option<u8>,
    // about nmi
    nmi_enabled: bool,
    //
    scp_b_writable: SystemControlPortB,
    // about pit
    pit: PIT,
}

impl Bundle {
    pub fn new() -> Self {
        Self {
            cmos_selected_reg: None,
            nmi_enabled: true,
            scp_b_writable: SystemControlPortB::empty(),
            pit: PIT::new(),
        }
    }

    fn read_system_control_a(&mut self, port: u16, access_size: u8) -> HyperResult<u32> {
        Ok(0)
    }

    fn write_system_control_a(&mut self, port: u16, access_size: u8, value: u32) -> HyperResult {
        Err(HyperError::NotSupported)
    }

    fn read_system_control_b(&mut self, port: u16, access_size: u8) -> HyperResult<u32> {
        let mut result = self.scp_b_writable;

        if self.pit.read_output(1)? {
            result |= SystemControlPortB::TIMER1_OUTPUT;
        }

        if self.pit.read_output(2)? {
            result |= SystemControlPortB::TIMER2_OUTPUT;
        }

        Ok(result.bits() as u32)
    }

    fn write_system_control_b(&mut self, port: u16, access_size: u8, value: u32) -> HyperResult {
        let value = SystemControlPortB::from_bits_truncate(value as u8) & !SystemControlPortB::READONLY_MASK;

        self.pit.set_enabled(2, value.contains(SystemControlPortB::TIMER2_ENABLED))?;
        self.scp_b_writable = value;

        Ok(())
    }

    fn read_cmos(&mut self, port: u16, access_size: u8) -> HyperResult<u32> {
        if port == PORT_CMOS_ADDRESS {
            Err(HyperError::NotSupported)
        } else {
            match self.cmos_selected_reg {
                None => Err(HyperError::InvalidParam),
                Some(selected) => {
                    self.cmos_selected_reg = None;
                    debug!("cmos read from reg {:#x} ignored", selected);
                    Ok(0)
                },
            }
        }
    }

    fn write_cmos(&mut self, port: u16, access_size: u8, value: u32) -> HyperResult {
        if port == PORT_CMOS_ADDRESS {
            self.cmos_selected_reg = Some((value & 0x7f) as u8);
            self.nmi_enabled = (value & 0x80) == 0;

            Ok(())
        } else { // port == PORT_CMOS_DATA
            match self.cmos_selected_reg {
                None => Err(HyperError::InvalidParam),
                Some(selected) => {
                    self.cmos_selected_reg = None;
                    debug!("cmos write to reg {:#x}(value {:#x}) ignored", selected, value);
                    Ok(())
                },
            }
        }
    }

    fn read_pit(&mut self, port: u16, access_size: u8) -> HyperResult<u32> {
        // debug!("pit read, port {port:#x}");

        if port == PORT_PIT_COMMAND {
            Ok(0)
        } else {
            self.pit.read((port - PORT_PIT_CHANNEL_DATA_BASE) as u8).map(|v| v as u32)
        }
    }

    fn write_pit(&mut self, port: u16, access_size: u8, value: u32) -> HyperResult {
        let value = value as u8;

        // debug!("pit write, port {port:#x}, value {value:#x}");

        if port == PORT_PIT_COMMAND {
            self.pit.command(value.get_bits(6..8), value.get_bits(4..6), value.get_bits(1..4), value.get_bit(0))
        } else {
            self.pit.write((port - PORT_PIT_CHANNEL_DATA_BASE) as u8, value)
        }
    }
}

// following are proxies


pmio_proxy_struct!(PORT_SYSTEM_CONTROL_A, PORT_SYSTEM_CONTROL_A, BundleSystemControlPortAProxy, Bundle, read_system_control_a, write_system_control_a);
pmio_proxy_struct!(PORT_SYSTEM_CONTROL_B, PORT_SYSTEM_CONTROL_B, BundleSystemControlPortBProxy, Bundle, read_system_control_b, write_system_control_b);
pmio_proxy_struct!(PORT_CMOS_ADDRESS, PORT_CMOS_DATA, BundleCMOSProxy, Bundle, read_cmos, write_cmos);
pmio_proxy_struct!(PORT_PIT_CHANNEL_DATA_BASE, PORT_PIT_COMMAND, BundlePITProxy, Bundle, read_pit, write_pit);

impl Bundle {
    pmio_proxy_factory!(proxy_system_control_a, BundleSystemControlPortAProxy);
    pmio_proxy_factory!(proxy_system_control_b, BundleSystemControlPortBProxy);
    pmio_proxy_factory!(proxy_cmos, BundleCMOSProxy);
    pmio_proxy_factory!(proxy_pit, BundlePITProxy);
}
