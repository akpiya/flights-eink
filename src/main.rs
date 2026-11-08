#![no_std]
#![no_main]

mod display;
mod encoder;
mod flight;
mod screen;

use display::Display;
use encoder::RotaryEncoder;

use defmt::info;

esp_bootloader_esp_idf::esp_app_desc!();


#[embassy_executor::task]
async fn run(mut runner: Runner<'static, Interface>) {

}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    info!("Initializing");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let encoder = RotaryEncoder::new(
        peripherals.GPI01,
        peripherals.GPI02,
        peripherals.GPI03,
    );

    let display = Display::new(
        peripherals.GPI04,
        peripherals.GPI05,
        peripherals,GPI06,
        peripherals.GPI07,
        peripherals.GPI08,
        peripherlas.GPI09,
        peripherals.SPI2,
    );

    loop {
    }
}

