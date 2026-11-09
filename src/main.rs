#![no_std]
#![no_main]

mod display;
mod encoder;
mod screen;
mod map;

use defmt::info;
use embassy_executor::Spawner;

use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;

use display::Display;
use encoder::RotaryEncoder;
use map::Map;
use screen::{Color, Screen, HEIGHT, WIDTH};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    info!("Initializing");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // The async executor runs on esp-rtos's scheduler, which must be started
    // before the first `.await`.
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let _encoder = RotaryEncoder::new(
        peripherals.GPIO27, // SW  (D27)
        peripherals.GPIO26, // DT  (D26)
        peripherals.GPIO25, // CLK (D25)
    );

    let mut display = Display::new(
        peripherals.GPIO17, // DC   (TX2)
        peripherals.GPIO16, // RST  (RX2)
        peripherals.GPIO5,  // CS   (D5)
        peripherals.GPIO18, // SCLK (D18)
        peripherals.GPIO23, // DIN  (D23)
        peripherals.GPIO21, // PWR  (D21)
        peripherals.GPIO4,  // BUSY (D4)
        peripherals.SPI2,
    );

    info!("Boot marker (absorbs the first undecodable defmt frame)");
    let mut screen = Screen::new();
    draw_sample_airplane(&mut screen); // TEMPORARY: contrast check
    let map = Map::new();
    display.init().unwrap();
    info!("Display initialized");
    display.show(&map.background(), &screen).unwrap();
    info!("Map shown");

    loop {
        core::future::pending::<()>().await;
    }
}

/// TEMPORARY: a ~150 px black airplane silhouette (nose up) in the middle of the
/// screen, to judge how the overlay color contrasts with the map.
fn draw_sample_airplane(screen: &mut Screen) {
    let (center_row, center_col) = (HEIGHT as i32 / 2, WIDTH as i32 / 2);
    for row in 0..HEIGHT as i32 {
        for col in 0..WIDTH as i32 {
            let (dx, dy) = ((col - center_col).abs(), row - center_row);
            // Distance back from a swept line through the wing/tail roots.
            let swept = dy - dx * 2 / 5;
            let fuselage = dx <= 7 && (-75..=70).contains(&dy) && (dy > -68 || dx <= 4);
            let wings = dx <= 75 && (-12..=8).contains(&swept);
            let tail = dx <= 28 && (52..=62).contains(&swept) && dy <= 70;
            if fuselage || wings || tail {
                screen.set(row as usize, col as usize, Color::Black);
            }
        }
    }
}
