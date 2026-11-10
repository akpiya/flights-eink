#![no_std]
#![no_main]

mod display;
mod encoder;
mod flight;
mod screen;
mod map;

use core::ffi::CStr;

use defmt::info;
use embassy_executor::Spawner;
use embassy_net::{Runner, StackResources};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    ram,
    rng::{Trng, TrngSource},
    timer::timg::TimerGroup,
};
use esp_println as _;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
    sta::StationConfig,
};
use mbedtls_rs::{Certificate, Tls, X509};
use static_cell::StaticCell;

use display::Display;
use encoder::RotaryEncoder;
use flight::FlightClient;
use map::Map;
use screen::Screen;

esp_bootloader_esp_idf::esp_app_desc!();

// Set in the gitignored .env, which must be sourced before building.
const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

// ISRG Root X1 and X2: adsb.lol's Let's Encrypt chain leads to both.
const ROOT_CERTS: &str = concat!(include_str!("../certs/adsb-lol-roots.pem"), "\0");

static TLS_RNG: StaticCell<Trng> = StaticCell::new();
static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    info!("Initializing");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 96 * 1024);
    esp_alloc::heap_allocator!(size: 16 * 1024);

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
    let map = Map::new();
    display.init().unwrap();
    info!("Display initialized");
    display.show(&map.background(), &screen).unwrap();
    info!("Map shown");

    // TEMPORARY: fetch flights around Austin to check the API works on the device.
    // TLS needs unpredictable random numbers, which the TRNG source provides.
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let trng = TLS_RNG.init(Trng::try_new().expect("TRNG source is enabled"));
    let seed = u64::from(trng.random()) << 32 | u64::from(trng.random());

    let station = Config::Station(
        StationConfig::default()
            .with_ssid(WIFI_SSID.try_into().expect("SSID is at most 32 bytes"))
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                WIFI_PASSWORD.try_into().expect("password is at most 64 bytes"),
            )),
    );
    let controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station),
    )
    .expect("WiFi starts");
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(Default::default()),
        STACK_RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(stay_connected(controller).expect("task is free"));
    spawner.spawn(run_network(runner).expect("task is free"));
    stack.wait_config_up().await;
    info!("Got IP: {}", stack.config_v4().map(|config| config.address));

    let tls = Tls::new(trng).expect("only TLS instance");
    let root_certs = CStr::from_bytes_with_nul(ROOT_CERTS.as_bytes()).expect("one trailing NUL");
    let ca = Certificate::new(X509::PEM(root_certs)).expect("root certificates parse");
    let mut flights = FlightClient::new(stack, ca, tls.reference());

    loop {
        match flights.get_flights(30.2672, -97.7431, 25).await {
            Ok(flights) => {
                info!("{} flights", flights.len());
                for flight in &flights {
                    info!(
                        "{} at {}, {}: {} kt, {} deg",
                        flight.flight.as_deref(),
                        flight.lat,
                        flight.lon,
                        flight.gs,
                        flight.track
                    );
                }
            }
            Err(error) => info!("get_flights failed: {}", error),
        }
        Timer::after(Duration::from_secs(10)).await;
    }
}

#[embassy_executor::task]
async fn stay_connected(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(_) => {
                info!("WiFi connected");
                let _ = controller.wait_for_disconnect_async().await;
                info!("WiFi disconnected");
            }
            Err(error) => info!("WiFi connect failed: {}", error),
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn run_network(mut runner: Runner<'static, Interface>) {
    runner.run().await
}
