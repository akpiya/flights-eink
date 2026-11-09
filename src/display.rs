use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{
        interconnect::PeripheralOutput, Input, InputConfig, InputPin, Level, Output,
        OutputConfig, OutputPin,
    },
    spi::master::{Instance, Spi},
    time::{Duration, Instant, Rate},
};

use crate::screen::Screen;

// The manual's section 10.2 lists command 0x24 followed by 81,600 bytes to "Load BW image":
// https://files.waveshare.com/wiki/13.3inch-e-Paper-HAT-%28B%29/13.3inch-e-Paper-%28B%29-user-manual.pdf#page=25
enum Command {
    WriteImage = 0x24,
    ImageUpdate = 0x22,
    MasterActivation = 0x20,
    DeepSleep = 0x10,
}

#[derive(Debug)]
pub enum DisplayError {
    Spi(esp_hal::spi::Error),
    BusyTimeout,
}

pub struct Display {
    cs: Output<'static>,
    dc: Output<'static>,
    rst: Output<'static>,
    pwr: Output<'static>,
    busy: Input<'static>,
    spi: Spi<'static, Blocking>,
    delay: Delay,
}

impl Display {
    pub fn new(
        dc: impl OutputPin + 'static,
        rst: impl OutputPin + 'static,
        cs: impl OutputPin + 'static,
        clk: impl PeripheralOutput<'static>,
        din: impl PeripheralOutput<'static>,
        pwr: impl OutputPin + 'static,
        busy: impl InputPin + 'static,
        spi_peripheral: impl Instance + 'static,
    ) -> Self {
        let config = esp_hal::spi::master::Config::default().with_frequency(Rate::from_mhz(8));

        Self {
            dc: Output::new(dc, Level::Low, OutputConfig::default()),
            cs: Output::new(cs, Level::Low, OutputConfig::default()),
            rst: Output::new(rst, Level::High, OutputConfig::default()),
            pwr: Output::new(pwr, Level::High, OutputConfig::default()),
            busy: Input::new(busy, InputConfig::default()),
            spi: Spi::new(spi_peripheral, config)
                .expect("8 MHz SPI configuration is always valid")
                .with_sck(clk)
                .with_mosi(din),
            delay: Delay::new(),
        }
    }

    fn send_command(&mut self, cmd: Command) -> Result<(), DisplayError> {
        self.dc.set_level(Level::Low);
        self.cs.set_level(Level::Low);
        self.spi.write(&[cmd as u8]).map_err(DisplayError::Spi)?;
        self.cs.set_level(Level::High);
        Ok(())
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), DisplayError> {
        self.dc.set_level(Level::High);
        self.cs.set_level(Level::Low);
        self.spi.write(data).map_err(DisplayError::Spi)?;
        self.cs.set_level(Level::High);
        Ok(())
    }

    fn wait_until_idle(&mut self) -> Result<(), DisplayError> {
        let start = Instant::now();
        while self.busy.is_high() {
            if start.elapsed() > Duration::from_secs(10) {
                return Err(DisplayError::BusyTimeout);
            }
            self.delay.delay_millis(20);
        }
        self.delay.delay_millis(20);
        Ok(())
    }

   fn hardware_reset(&mut self) {
        self.rst.set_level(Level::High);
        self.delay.delay_millis(100);
        self.rst.set_level(Level::Low);
        self.delay.delay_millis(2);
        self.rst.set_level(Level::High);
        self.delay.delay_millis(100);
    }

    pub fn init(&mut self) -> Result<(), DisplayError> {
        self.hardware_reset();
        self.wait_until_idle()?;
        Ok(())
    }

    pub fn show(&mut self, screen: &Screen) -> Result<(), DisplayError> {
        self.send_command(Command::WriteImage)?;
        self.send_data(screen.panel_bytes())?;

        self.send_command(Command::ImageUpdate)?;
        self.send_data(&[0xF7])?;
        self.send_command(Command::MasterActivation)?;
        self.wait_until_idle()
    }

    pub fn sleep(&mut self) -> Result<(), DisplayError> {
        self.send_command(Command::DeepSleep)?;
        self.send_data(&[0x03])?;
        self.delay.delay_millis(100);
        Ok(())
    }
}
