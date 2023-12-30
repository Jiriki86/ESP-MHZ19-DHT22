use core::fmt;
use embedded_hal::delay::DelayUs;
use embedded_hal::digital::{InputPin, OutputPin, PinState};

/// DHT readout data
#[derive(Debug, Clone, Copy)]
pub struct ReadoutData {
    temperature: f32,
    humidity: f32,
}

impl ReadoutData {
    /// Returns the ambient humidity in the range of 0..100%
    pub fn humidity(&self) -> f32 {
        self.humidity
    }

    /// Returns the ambient temperature in degree celsius
    pub fn temperature(&self) -> f32 {
        self.temperature
    }
}

/// Error enum for dht sensor readout
#[derive(Debug, Clone)]
pub enum DhtError<HalError> {
    // dht is not found at given gpio pin
    NotFoundOnGPio,
    // timeout while reading data
    ReadTimeout,
    // received a low-level hal error while reading or writing io-pin
    PinError(HalError),
    // checksum error in received data
    CheckSum(u8, u8),
}

impl<HalError> From<HalError> for DhtError<HalError> {
    fn from(error: HalError) -> Self {
        DhtError::PinError(error)
    }
}

impl<HE: fmt::Debug> fmt::Display for DhtError<HE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DhtError::*;
        match self {
            NotFoundOnGPio => write!(f, "DHT device not found on gpio pin"),
            ReadTimeout => write!(f, "timeout while reading"),
            PinError(err) => write!(f, "HAL pin error: {:?}", err),
            CheckSum(exp, act) => write!(f, "Checksum error: {:x} vs {:x}", exp, act),
        }
    }
}

#[cfg(feature = "std")]
impl<HE: fmt::Debug> std::error::Error for DhtError<HE> {}

/// A Dht22 sensor
pub struct Dht22<HalError, D: DelayUs, P: InputPin<Error = HalError> + OutputPin<Error = HalError>>
{
    delay: D,
    pin: P,
}

impl<HE, D: DelayUs, P: InputPin<Error = HE> + OutputPin<Error = HE>> Dht22<HE, D, P> {
    pub fn new(delay: D, pin: P) -> Self {
        Self { delay, pin }
    }

    fn parse_buffer(buf: &[u8]) -> (f32, f32) {
        let humidity = (((buf[0] as u16) << 8) + buf[1] as u16) as f32 / 10.0;
        let mut temp = ((((buf[2] & 0x7f) as u16) << 8) | buf[3] as u16) as f32 / 10.0;
        if buf[2] & 0x80 != 0 {
            temp = -temp;
        }
        (humidity, temp)
    }

    pub fn read(&mut self) -> Result<ReadoutData, DhtError<HE>> {
        // wake up dht22
        self.pin.set_low()?;
        self.delay.delay_us(3000);
        // ask for data
        self.pin.set_high()?;
        self.delay.delay_us(25);

        // wait for dht to signal that data is ready
        self.wait_for_state(PinState::High, 85, DhtError::NotFoundOnGPio)?;
        self.wait_for_state(PinState::Low, 85, DhtError::NotFoundOnGPio)?;

        // read the 40 data bits
        let mut buf: [u8; 5] = [0; 5];
        for bit in 0..40 {
            // wait for next high state
            self.wait_for_state(PinState::High, 55, DhtError::ReadTimeout)?;
            // check how long it takes to go low again
            let elapsed = self.wait_for_state(PinState::Low, 70, DhtError::ReadTimeout)?;
            // a logical '1' will take more than 30us to go low again
            if elapsed > 30 {
                let byte = bit / 8;
                let shift = 7 - bit % 8;
                buf[byte] |= 1 << shift;
            }
        }

        let checksum = (buf[0..=3]
            .iter()
            .fold(0u16, |accum, next| accum + *next as u16)
            & 0xff) as u8;
        if checksum == buf[4] {
            let (humidity, temp) = Self::parse_buffer(&buf);
            return Ok(ReadoutData {
                humidity,
                temperature: temp,
            });
        }
        Err(DhtError::CheckSum(checksum, buf[4]))
    }

    fn wait_for_state(
        &mut self,
        state: PinState,
        timeout_us: u32,
        timeout_error: DhtError<HE>,
    ) -> Result<u32, DhtError<HE>> {
        let state_test = || match state {
            PinState::High => self.pin.is_high(),
            PinState::Low => self.pin.is_low(),
        };

        for elapsed_time in 0..=timeout_us {
            if state_test()? {
                return Ok(elapsed_time);
            }
            self.delay.delay_us(1);
        }
        Err(timeout_error)
    }
}
