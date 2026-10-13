use esp_hal::{
    Blocking,
    gpio::{
        interconnect::PeripheralOutput,
        Input, InputConfig, InputPin, Level, Output, OutputConfig, OutputPin,
    },
    spi::master::{Instance, Spi},
};

pub struct Display {
    cs:   Output<'static>,
    dc:   Output<'static>,
    rst:  Output<'static>,
    pwr:  Output<'static>,
    busy: Input<'static>,
    spi:  Spi<'static, Blocking>,
}

impl Display {
    pub fn new(
        dc: impl OutputPin + 'static,
        rst: impl OutputPin + 'static,
        cs:  impl OutputPin + 'static,
        clk: impl PeripheralOutput<'static>,
        din: impl PeripheralOutput<'static>,
        pwr: impl OutputPin + 'static,
        busy: impl InputPin + 'static,
        spi_peripheral: impl Instance + 'static,
    ) -> Self {
        Self {
            dc: Output::new(dc, Level::Low, OutputConfig::default()),
            cs: Output::new(cs, Level::Low, OutputConfig::default()),
            rst: Output::new(rst, Level::High, OutputConfig::default()),
            pwr: Output::new(pwr, Level::High, OutputConfig::default()),
            busy: Input::new(busy, InputConfig::default()),
            spi: Spi::new(
                spi_peripheral,
                esp_hal::spi::master::Config::default(),
            ).unwrap().with_sck(clk).with_mosi(din)
        }
    }
}
