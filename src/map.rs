use core::f64::consts::PI;

use libm::{asinh, atan, ceil, cos, hypot, sinh, tan};

use crate::screen::{HEIGHT, PANEL_BYTES, WIDTH};

/// Each record is the map's zoom, its top-left world pixel as two little-endian
/// i32s, then its image (see `build.rs`).
const HEADER_BYTES: usize = 1 + 2 * 4;
const MAP_RECORD_BYTES: usize = HEADER_BYTES + PANEL_BYTES;
static MAP_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/maps.bin"));

const TILE_SIZE: f64 = 256.0;
const EARTH_CIRCUMFERENCE_M: f64 = 40_075_016.686;
const METERS_PER_NM: f64 = 1852.0;

pub struct Map {
    map_selection: usize,
}

impl Map {
    pub const fn new() -> Self {
        Self { map_selection: 0 }
    }

    pub fn select_zoom(&mut self, zoom: u8) -> bool {
        if let Some(index) = map_records().position(|record| record[0] == zoom) {
            self.map_selection = index;
            true
        } else {
            false
        }
    }

    /// The map image, 8 pixels per byte with 1 for a line. Lives in flash.
    pub fn background(&self) -> &'static [u8] {
        &self.record()[HEADER_BYTES..]
    }

    /// Where a coordinate falls on the map, in pixels from its top-left corner.
    pub fn position(&self, latitude: f32, longitude: f32) -> (f32, f32) {
        let (x, y) = world_pixel(latitude.into(), longitude.into(), self.zoom());
        let (left, top) = self.top_left();
        ((x - left) as f32, (y - top) as f32)
    }

    /// The coordinate at the map's center.
    pub fn center(&self) -> (f32, f32) {
        let (left, top) = self.top_left();
        let world_size = world_size(self.zoom());
        let x = left + WIDTH as f64 / 2.0;
        let y = top + HEIGHT as f64 / 2.0;
        let longitude = x / world_size * 360.0 - 180.0;
        let latitude = atan(sinh(PI * (1.0 - 2.0 * y / world_size))).to_degrees();
        (latitude as f32, longitude as f32)
    }

    /// The distance from the map's center to its corners, rounded up to whole
    /// nautical miles.
    pub fn radius_nm(&self) -> u32 {
        let (latitude, _) = self.center();
        // Web Mercator stretches distances by 1 / cos(latitude).
        let meters_per_pixel = EARTH_CIRCUMFERENCE_M * cos(f64::from(latitude).to_radians())
            / world_size(self.zoom());
        let half_diagonal = hypot(WIDTH as f64 / 2.0, HEIGHT as f64 / 2.0);
        ceil(half_diagonal * meters_per_pixel / METERS_PER_NM) as u32
    }

    fn record(&self) -> &'static [u8] {
        map_records()
            .nth(self.map_selection)
            .expect("map selection is valid")
    }

    fn zoom(&self) -> u8 {
        self.record()[0]
    }

    fn top_left(&self) -> (f64, f64) {
        let header = self.record();
        let read = |at: usize| {
            f64::from(i32::from_le_bytes(header[at..at + 4].try_into().expect("4 bytes")))
        };
        (read(1), read(5))
    }
}

fn map_records() -> impl Iterator<Item = &'static [u8]> {
    MAP_BUNDLE.chunks_exact(MAP_RECORD_BYTES)
}

fn world_size(zoom: u8) -> f64 {
    TILE_SIZE * f64::from(1u32 << zoom)
}

/// The same Web Mercator "world pixel" as `build.rs` uses. f64 because a world
/// pixel at zoom 13 runs into the millions, beyond what f32 holds to a pixel.
fn world_pixel(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    let world_size = world_size(zoom);
    let x = (longitude + 180.0) / 360.0 * world_size;
    let y = (1.0 - asinh(tan(latitude.to_radians())) / PI) / 2.0 * world_size;
    (x, y)
}
