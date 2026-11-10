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

use crate::{
    map::GrayImage,
    screen::{HEIGHT, PANEL_BYTES, Screen, WIDTH},
};

// The manual's section 10.2 lists command 0x24 followed by 81,600 bytes to "Load BW image":
// https://files.waveshare.com/wiki/13.3inch-e-Paper-HAT-%28B%29/13.3inch-e-Paper-%28B%29-user-manual.pdf#page=25
//
// The init sequence and 4-gray mode follow Waveshare's reference driver for the 13.3" K:
// https://github.com/waveshareteam/e-Paper/blob/master/RaspberryPi_JetsonNano/c/lib/e-Paper/EPD_13in3k.c
enum Command {
    DriverOutputControl = 0x01,
    GateVoltage = 0x03,
    SourceVoltage = 0x04,
    BoosterSoftStart = 0x0C,
    DeepSleep = 0x10,
    DataEntryMode = 0x11,
    SoftwareReset = 0x12,
    TemperatureSensor = 0x18,
    MasterActivation = 0x20,
    ImageUpdate = 0x22,
    WriteImage = 0x24,
    WriteRedImage = 0x26,
    WriteVcom = 0x2C,
    WriteLut = 0x32,
    BorderWaveform = 0x3C,
    RamXWindow = 0x44,
    RamYWindow = 0x45,
    RamXCounter = 0x4E,
    RamYCounter = 0x4F,
}

// Last pixel column / row, as the little-endian byte pairs the controller expects.
const X_END: [u8; 2] = ((WIDTH - 1) as u16).to_le_bytes();
const Y_END: [u8; 2] = ((HEIGHT - 1) as u16).to_le_bytes();

/// Waveshare's 4-gray waveform (`LUT_DATA_4Gray`): 105 bytes of LUT for command 0x32,
/// then gate voltage, three source voltages and VCOM.
const LUT_4GRAY: [u8; 110] = [
    0x80, 0x48, 0x4A, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x0A, 0x48, 0x68, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x88, 0x48, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0xA8, 0x48, 0x45, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x07, 0x23, 0x17, 0x02, 0x00, //
    0x05, 0x01, 0x05, 0x01, 0x02, //
    0x08, 0x02, 0x01, 0x04, 0x04, //
    0x00, 0x02, 0x00, 0x02, 0x01, //
    0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x01, //
    0x22, 0x22, 0x22, 0x22, 0x22, //
    0x17, 0x41, 0xA8, 0x32, 0x30, //
];

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
        // BUSY can take a moment to rise after a command (notably right after the
        // panel powers up), so give it a chance before treating low as "idle".
        let rise_start = Instant::now();
        while self.busy.is_low() && rise_start.elapsed() < Duration::from_millis(200) {
            self.delay.delay_millis(1);
        }

        let start = Instant::now();
        defmt::info!("BUSY at wait start: high={}", self.busy.is_high());
        while self.busy.is_high() {
            if start.elapsed() > Duration::from_secs(10) {
                return Err(DisplayError::BusyTimeout);
            }
            self.delay.delay_millis(20);
        }
        defmt::info!("BUSY low after {} ms", start.elapsed().as_millis());
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

    fn command(&mut self, cmd: Command, data: &[u8]) -> Result<(), DisplayError> {
        self.send_command(cmd)?;
        self.send_data(data)
    }

    /// Initializes the panel in 4-gray mode.
    pub fn init(&mut self) -> Result<(), DisplayError> {
        self.init_controller(0x00)?;

        // Load the 4-gray waveform in place of the built-in black/white one.
        self.command(Command::WriteLut, &LUT_4GRAY[..105])?;
        self.command(Command::GateVoltage, &LUT_4GRAY[105..106])?;
        self.command(Command::SourceVoltage, &LUT_4GRAY[106..109])?;
        self.command(Command::WriteVcom, &LUT_4GRAY[109..110])?;
        self.wait_until_idle()
    }

    /// Resets the controller and sets up the panel geometry. The software reset also
    /// restores the controller's built-in black/white waveform.
    fn init_controller(&mut self, border: u8) -> Result<(), DisplayError> {
       self.pwr.set_level(Level::High);
        self.delay.delay_millis(500);
        self.hardware_reset();
        self.delay.delay_millis(100);
        self.wait_until_idle()?;

        self.send_command(Command::SoftwareReset)?;
        self.wait_until_idle()?;

        self.command(Command::BoosterSoftStart, &[0xAE, 0xC7, 0xC3, 0xC0, 0x80])?;
        self.command(Command::DriverOutputControl, &[Y_END[0], Y_END[1], 0x00])?;
        // X and Y both increment, so bytes fill rows left-to-right, top-to-bottom.
        self.command(Command::DataEntryMode, &[0x03])?;
        self.command(Command::RamXWindow, &[0x00, 0x00, X_END[0], X_END[1]])?;
        self.command(Command::RamYWindow, &[0x00, 0x00, Y_END[0], Y_END[1]])?;
        self.command(Command::BorderWaveform, &[border])?;
        self.command(Command::TemperatureSensor, &[0x80])?;
        self.command(Command::RamXCounter, &[0x00, 0x00])?;
        self.command(Command::RamYCounter, &[0x00, 0x00])?;
        self.wait_until_idle()
    }

    /// Prepares the panel for storage: clears it to white with a normal black/white
    /// full refresh (an image left on the panel for a long time can burn in), puts
    /// the controller into deep sleep, and switches off the panel's power.
    pub fn clear_and_sleep(&mut self) -> Result<(), DisplayError> {
        self.init_controller(0x01)?;
        let white = [0xFF; WIDTH / 8];
        for command in [Command::WriteImage, Command::WriteRedImage] {
            self.send_command(command)?;
            for _ in 0..HEIGHT {
                self.send_data(&white)?;
            }
        }
        self.send_command(Command::ImageUpdate)?;
        self.send_data(&[0xF7])?;
        self.send_command(Command::MasterActivation)?;
        self.wait_until_idle()?;

        self.sleep()?;
        self.pwr.set_level(Level::Low);
        Ok(())
    }

    /// Shows a 4-gray background with the screen's black pixels drawn on top.
    ///
    /// Black is 1 in both RAM planes, so each overlay pixel that is black
    /// (0 in `Screen`) is OR-ed into both planes as the rows are streamed out.
    pub fn show(&mut self, background: &GrayImage, overlay: &Screen) -> Result<(), DisplayError> {
        let overlay = overlay.panel_bytes();
        for (command, plane) in [
            (Command::WriteImage, background.bw_ram),
            (Command::WriteRedImage, background.red_ram),
        ] {
            assert_eq!(plane.len(), PANEL_BYTES);
            self.send_command(command)?;
            let mut row = [0u8; WIDTH / 8];
            for (plane_row, overlay_row) in plane.chunks_exact(row.len()).zip(overlay.chunks_exact(row.len())) {
                for ((out, &map), &screen) in row.iter_mut().zip(plane_row).zip(overlay_row) {
                    *out = map | !screen;
                }
                self.send_data(&row)?;
            }
        }

        self.send_command(Command::ImageUpdate)?;
        self.send_data(&[0xC7])?;
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
