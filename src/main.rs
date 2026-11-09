#![no_std]
#![no_main]

mod display;
mod encoder;
mod flight;
mod screen;
mod map;

use defmt::info;
use embassy_executor::Spawner;

use esp_backtrace as _;
use esp_println as _;

use display::Display;
use encoder::RotaryEncoder;
use map::Map;
use screen::Screen;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    info!("Initializing");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let _encoder = RotaryEncoder::new(
        peripherals.GPIO1,
        peripherals.GPIO2,
        peripherals.GPIO3,
    );

    let mut display = Display::new(
        peripherals.GPIO4,
        peripherals.GPIO5,
        peripherals.GPIO6,
        peripherals.GPIO7,
        peripherals.GPIO8,
        peripherals.GPIO9,
        peripherals.GPIO13,
        peripherals.SPI2,
    );

    let mut screen = Screen::new();
    Map::new().draw_background(&mut screen);
    display.init().unwrap();
    display.show(&screen).unwrap();

    loop {
        core::future::pending::<()>().await;
    }
}
