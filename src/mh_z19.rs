use core::fmt;
use embedded_io::{Read, Write};

#[derive(Debug)]
pub enum MHz19Error<HE> {
    /// received and calculated checksums do not match
    Checksum(u8, u8),
    /// Error of underlying IO
    HalError(HE),
}

impl<HE> From<HE> for MHz19Error<HE> {
    fn from(error: HE) -> Self {
        MHz19Error::HalError(error)
    }
}

impl<HE: fmt::Debug> fmt::Display for MHz19Error<HE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MHz19Error::*;
        match self {
            Checksum(exp, act) => write!(f, "Checksum error: 0x{:x} vs 0x{:x}", exp, act),
            HalError(err) => write!(f, "HAL error: {:?}", err),
        }
    }
}

#[cfg(feature = "std")]
impl<HE: fmt::Debug> std::error::Error for MHz19Error<HE> {}

pub struct MHz19<HE, U: Read<Error = HE> + Write<Error = HE>> {
    uart: U,
}

impl<HE, U: Read<Error = HE> + Write<Error = HE>> MHz19<HE, U> {
    pub fn new(uart: U) -> Self {
        Self { uart }
    }

    fn calculate_checksum(data: &[u8]) -> u8 {
        let mut checksum = 0;
        for i in 1..=7 {
            checksum += data[i] as i16;
        }
        checksum = 0xff - checksum;
        (checksum + 1) as u8
    }

    pub fn read_co2(&mut self) -> Result<i32, MHz19Error<HE>> {
        let read_cmd = [0xFF, 0x1, 0x86, 0, 0, 0, 0, 0, 0x79];
        self.uart.write(&read_cmd)?;

        let mut response: [u8; 9] = [0; 9];
        self.uart.read(&mut response)?;

        let checksum = Self::calculate_checksum(&response);
        if checksum != response[8] {
            return Err(MHz19Error::Checksum(checksum, response[8]));
        }

        Ok(((response[2] as i32) << 8) + response[3] as i32)
    }

    pub fn enable_auto_calibration(&mut self, enable: bool) -> Result<(), MHz19Error<HE>> {
        let mut cmd = [0xFF, 0x1, 0x79, 0, 0, 0, 0, 0, 0];
        if enable {
            cmd[3] = 0xA0;
        }
        cmd[8] = Self::calculate_checksum(&cmd);
        self.uart.write(&cmd)?;

        Ok(())
    }
}
