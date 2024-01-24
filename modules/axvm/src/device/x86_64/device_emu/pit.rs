use bit_field::BitField;
use crate::{Result as HyperResult, Error as HyperError, HyperCraftHal, HyperCraftHalImpl};
use libax::time::current_time_nanos;

pub const PIT_FREQ: u32 = 1_193182;
pub const PIT_CHANNEL_COUNT: usize = 3;
pub const NANOS_PER_SEC: u64 = 1_000_000_000;

enum PITChannelAccessMode {
    LowOnly,
    HighOnly,
    LowThenHigh,
    Invalid,
}

impl TryFrom<u8> for PITChannelAccessMode {
    type Error = HyperError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Err(HyperError::NotSupported),
            1 => Ok(Self::LowOnly),
            2 => Ok(Self::HighOnly),
            3 => Ok(Self::LowThenHigh),
            _ => Err(HyperError::InvalidParam),
        }
    }
}

enum PITChannelOpMode {
    OneShot,
    Invalid,
}

impl TryFrom<u8> for PITChannelOpMode {
    type Error = HyperError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::OneShot),
            _ => Err(HyperError::NotSupported),
        }
    }
}

struct PITChannel {
    reload: u32, // 16-bit is enough for counter and reload but ...
    reload_low_written: bool,
    start_nanos: u64,
    started: bool,
    access_mode: PITChannelAccessMode,
    op_mode: PITChannelOpMode,
    low_read: bool,
}

impl PITChannel {
    fn new() -> Self {
        Self {
            reload: 0,
            reload_low_written: false,
            start_nanos: 0,
            started: false,
            access_mode: PITChannelAccessMode::Invalid,
            low_read: false,
            op_mode: PITChannelOpMode::Invalid,
        }
    }

    fn command(&mut self, access_mode: u8, op_mode: u8, bcd: bool) -> HyperResult {
        let access_mode: PITChannelAccessMode = access_mode.try_into()?;
        let op_mode: PITChannelOpMode = op_mode.try_into()?;

        if bcd {
            return Err(HyperError::NotSupported);
        }

        match op_mode {
            PITChannelOpMode::OneShot => {
                self.access_mode = access_mode;
                self.op_mode = op_mode;

                self.reload_low_written = false;
                self.started = false;
                
                Ok(())
            },
            _ => Err(HyperError::NotSupported),
        }
    }

    fn read_low_byte(&self) -> u8 {
        self.read_counter().get_bits(0..8) as u8
    }

    fn read_high_byte(&self) -> u8 {
        self.read_counter().get_bits(8..16) as u8
    }

    fn read(&mut self) -> HyperResult<u8> {
        match self.access_mode {
            PITChannelAccessMode::LowOnly => Ok(self.read_low_byte()),
            PITChannelAccessMode::HighOnly => Ok(self.read_high_byte()),
            PITChannelAccessMode::LowThenHigh => {
                self.low_read = !self.low_read;
                Ok(if self.low_read {
                    self.read_low_byte()
                } else {
                    self.read_high_byte()
                })
            },
            _ => Err(HyperError::BadState),
        }
    }

    fn restart(&mut self) {
        self.started = true;
        self.start_nanos = current_time_nanos();
    }

    fn write(&mut self, value: u8) -> HyperResult {
        match self.op_mode {
            PITChannelOpMode::OneShot => {
                match self.access_mode {
                    PITChannelAccessMode::LowOnly => {
                        self.reload.set_bits(0..8, value as u32);
                        self.restart();
                        Ok(())
                    },
                    PITChannelAccessMode::HighOnly => {
                        self.reload.set_bits(8..16, value as u32);
                        self.restart();
                        Ok(())
                    },
                    PITChannelAccessMode::LowThenHigh => {
                        if self.reload_low_written {
                            self.reload.set_bits(0..8, value as u32);
                        } else {
                            self.reload.set_bits(8..16, value as u32);
                            self.restart();
                        }

                        self.reload_low_written = !self.reload_low_written;
                        Ok(())
                    },
                    _ => Err(HyperError::BadState),
                }
            }
            _ => Err(HyperError::BadState),
        }
    }

    fn eclipsed_periods(&self) -> u64 {
        if self.started {
            let eclipsed_nanos = current_time_nanos() - self.start_nanos;
            ((eclipsed_nanos as u128 * PIT_FREQ as u128) / (NANOS_PER_SEC as u128)) as u64
        } else {
            0
        }
    }

    fn read_counter(&self) -> u16 {
        let eclipsed_periods = self.eclipsed_periods();
        let reload = self.reload as u64;

        ((reload - eclipsed_periods) & 0xffff) as u16
    }

    fn read_output(&self) -> bool {
        if self.started {
            self.eclipsed_periods() > self.reload as u64
        } else {
            false
        }
    }

    fn set_enabled(&self, enabled: bool) {
    }
}

/// Intel 8253/8254 Programmable Interval Timer (PIT) emulation
pub struct PIT {
    channels: [PITChannel; PIT_CHANNEL_COUNT],
}

impl PIT {
    pub fn new() -> Self {
        Self {
            channels: [PITChannel::new(), PITChannel::new(), PITChannel::new()],
        }
    }

    pub fn command(&mut self, channel: u8, access_mode: u8, op_mode: u8, bcd: bool) -> HyperResult {
        let channel = channel as usize;
        if channel >= PIT_CHANNEL_COUNT {
            Err(HyperError::InvalidParam)
        } else {
            self.channels[channel].command(access_mode, op_mode, bcd).or_else(|err| {
                warn!("PIT command (channel: {channel}, access_mode: {access_mode:#x}, op_mode: {op_mode:#x}, bcd: {bcd}) error: {err:?}, skipped");
                Ok(())
            })
        }
    }
    
    pub fn read(&mut self, channel: u8) -> HyperResult<u8> {
        let channel = channel as usize;
        if channel >= PIT_CHANNEL_COUNT {
            Err(HyperError::InvalidParam)
        } else {
            self.channels[channel].read().or_else(|err| {
                // warn!("PIT read (channel: {channel}) error: {err:?}, skipped");
                Ok(0)
            })
        }
    }

    pub fn write(&mut self, channel: u8, value: u8) -> HyperResult {
        let channel = channel as usize;
        if channel >= PIT_CHANNEL_COUNT {
            Err(HyperError::InvalidParam)
        } else {
            self.channels[channel].write(value).or_else(|err| {
                // warn!("PIT write (channel: {channel}, value: {value}) error: {err:?}, skipped");
                Ok(())
            })
        }
    }

    pub fn read_output(&mut self, channel: u8) -> HyperResult<bool> {
        let channel = channel as usize;
        if channel >= PIT_CHANNEL_COUNT {
            Err(HyperError::InvalidParam)
        } else {
            Ok(self.channels[channel].read_output())
        }
    }

    pub fn set_enabled(&mut self, channel: u8, enabled: bool) -> HyperResult {
        let channel = channel as usize;
        if channel >= PIT_CHANNEL_COUNT {
            Err(HyperError::InvalidParam)
        } else {
            Ok(self.channels[channel].set_enabled(enabled))
        }
    }
}
