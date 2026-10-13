use esp_hal::gpio::{Input, InputPin,InputConfig, Pull};

pub struct RotaryEncoder{
    sw: Input<'static>,
    dt: Input<'static>,
    clk: Input<'static>,
}

impl RotaryEncoder {
    pub fn new(
        sw_pin: impl InputPin + 'static,
        dt_pin: impl InputPin + 'static,
        clk_pin: impl InputPin + 'static,
    ) -> Self {
        Self {
            sw: Input::new(sw_pin, InputConfig::default().with_pull(Pull::Up)),
            dt: Input::new(dt_pin, InputConfig::default().with_pull(Pull::Up)),
            clk: Input::new(clk_pin, InputConfig::default().with_pull(Pull::Up)),
        }
    }
}
