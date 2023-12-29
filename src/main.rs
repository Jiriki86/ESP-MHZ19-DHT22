use anyhow::Result;
use esp_idf_svc::hal::delay::Delay;
use esp_idf_svc::hal::{
    gpio::AnyIOPin, gpio::PinDriver, peripherals::Peripherals, prelude::*, uart,
};
use std::{thread::sleep, time::Duration};

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
    fn new(delay: D, pin: P) -> Self {
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

    fn read(&mut self) -> Result<ReadoutData, DhtError<HE>> {
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

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("ESP started");

    let peripherals = Peripherals::take().unwrap();

    // lets blink an LED while we are running
    let mut led_pin = PinDriver::output(peripherals.pins.gpio2);

    // configure a uart port to read the co2 sensor data
    let config = uart::config::Config::default().baudrate(Hertz(9600));

    let uart: uart::UartDriver = uart::UartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio33,
        peripherals.pins.gpio32,
        Option::<AnyIOPin>::None,
        Option::<AnyIOPin>::None,
        &config,
    )
    .unwrap();

    // sleep before talking to dht22 for first time
    sleep(Duration::from_millis(100));

    // get io pin to talk to dht22
    let delay = Delay::new_default();
    let dht22_pin = PinDriver::input_output_od(peripherals.pins.gpio4).unwrap();
    let mut dht22 = Dht22::new(delay, dht22_pin);

    // TODO: create wifi connection and mqtt client

    loop {
        led_pin.as_mut().unwrap().toggle()?;
        // TODO: create library for sensor
        uart.write(&[0xFF, 0x1, 0x86, 0, 0, 0, 0, 0, 0x79]).unwrap();
        let mut response: [u8; 9] = [0; 9];
        uart.read(&mut response, 500).unwrap();
        let co2 = ((response[2] as i32) << 8) + response[3] as i32;
        let hum_and_temp = dht22.read();
        match hum_and_temp {
            Ok(val) => log::info!(
                "Temp: {:}, Hum: {:}, CO2: {:}",
                val.temperature(),
                val.humidity(),
                co2
            ),
            Err(err) => log::warn!("{}", err),
        }

        sleep(Duration::from_millis(2500));
    }
}
