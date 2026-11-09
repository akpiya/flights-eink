use std::{env, fs, path::{Path, PathBuf}};

use image::ImageReader;

const WIDTH: usize = 960;
const HEIGHT: usize = 680;
const PANEL_BYTES: usize = (WIDTH * HEIGHT) / 8;
const MAP_URL: &str = "https://tiles.stadiamaps.com/static_cacheable/stamen_toner.png";
const DEFAULT_ZOOMS: &str = "8,10,12,14";
const DEFAULT_LATITUDE: &str = "30.2672";
const DEFAULT_LONGITUDE: &str = "-97.7431";

fn main() {
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=STADIA_API_KEY");
    println!("cargo:rerun-if-env-changed=MAP_ZOOMS");
    println!("cargo:rerun-if-env-changed=MAP_LATITUDE");
    println!("cargo:rerun-if-env-changed=MAP_LONGITUDE");

    let api_key = env::var("STADIA_API_KEY").expect("set STADIA_API_KEY to download map images");
    let zooms = parse_zooms(&env::var("MAP_ZOOMS").unwrap_or_else(|_| DEFAULT_ZOOMS.into()));
    let latitude = parse_coordinate("MAP_LATITUDE", DEFAULT_LATITUDE, -90.0, 90.0);
    let longitude = parse_coordinate("MAP_LONGITUDE", DEFAULT_LONGITUDE, -180.0, 180.0);
    println!("cargo:rustc-env=MAP_LATITUDE={latitude}");
    println!("cargo:rustc-env=MAP_LONGITUDE={longitude}");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));

    let mut maps = Vec::with_capacity(zooms.len() * (1 + PANEL_BYTES));
    for zoom in zooms {
        let png = out_dir.join(format!("austin-{zoom}.png"));
        download_map(&api_key, zoom, latitude, longitude, &png);
        maps.push(zoom);
        maps.extend(convert_to_panel_bytes(&png));
    }

    fs::write(out_dir.join("maps.bin"), maps).expect("write map data bundle");
}

fn parse_coordinate(name: &str, default: &str, min: f32, max: f32) -> f32 {
    let value: f32 = env::var(name)
        .unwrap_or_else(|_| default.into())
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be a number"));
    assert!(
        value.is_finite() && (min..=max).contains(&value),
        "{name} must be between {min} and {max}"
    );
    value
}

fn parse_zooms(value: &str) -> Vec<u8> {
    let mut zooms: Vec<u8> = value
        .split(',')
        .map(|part| {
            part.trim()
                .parse()
                .expect("MAP_ZOOMS must be comma-separated integers")
        })
        .collect();
    assert!(
        !zooms.is_empty(),
        "MAP_ZOOMS must include at least one zoom level"
    );
    assert!(
        zooms.iter().all(|zoom| *zoom <= 18),
        "Stadia zoom levels must be from 0 to 18"
    );
    zooms.sort();
    assert!(
        zooms.windows(2).all(|pair| pair[0] != pair[1]),
        "MAP_ZOOMS cannot contain duplicates"
    );
    zooms
}

fn download_map(api_key: &str, zoom: u8, latitude: f32, longitude: f32, path: &Path) {
    let url = format!("{MAP_URL}?center={latitude},{longitude}&zoom={zoom}&size={WIDTH}x{HEIGHT}");
    let mut response = ureq::get(&url)
        .header("Authorization", &format!("Stadia-Auth {api_key}"))
        .call()
        .unwrap_or_else(|error| panic!("failed to download Austin map at zoom {zoom}: {error}"));
    let bytes = response
        .body_mut()
        .read_to_vec()
        .unwrap_or_else(|error| panic!("failed to read Austin map at zoom {zoom}: {error}"));
    fs::write(path, bytes).expect("write downloaded map image");
}

fn convert_to_panel_bytes(png: &Path) -> Vec<u8> {
    let image = ImageReader::open(png)
        .expect("open downloaded map image")
        .decode()
        .expect("decode downloaded map image")
        .to_luma8();
    assert!(
        image.dimensions() == (WIDTH as u32, HEIGHT as u32),
        "map image must be {WIDTH}x{HEIGHT}"
    );

    let mut packed = Vec::with_capacity(PANEL_BYTES);
    for pixels in image.as_raw().chunks_exact(8) {
        let mut byte = 0;
        for (bit, pixel) in pixels.iter().enumerate() {
            if *pixel >= 128 {
                byte |= 0b1000_0000 >> bit;
            }
        }
        packed.push(byte);
    }
    assert_eq!(packed.len(), PANEL_BYTES);
    packed
}
