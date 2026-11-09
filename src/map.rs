use crate::screen::PANEL_BYTES;

/// Each record is a zoom byte followed by the map's two 4-gray RAM planes.
const MAP_RECORD_BYTES: usize = 1 + 2 * PANEL_BYTES;
static MAP_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/maps.bin"));

pub const MAP_LATITUDE: &str = env!("MAP_LATITUDE");
pub const MAP_LONGITUDE: &str = env!("MAP_LONGITUDE");

/// A 4-gray map image as the two bit planes the panel's RAMs take
/// (see `build.rs` for how each tone is encoded). Lives in flash.
pub struct GrayImage {
    pub bw_ram: &'static [u8],
    pub red_ram: &'static [u8],
}

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

    pub fn background(&self) -> GrayImage {
        let record = map_records()
            .nth(self.map_selection)
            .expect("map selection is valid");
        let (bw_ram, red_ram) = record[1..].split_at(PANEL_BYTES);
        GrayImage { bw_ram, red_ram }
    }
}

fn map_records() -> impl Iterator<Item = &'static [u8]> {
    MAP_BUNDLE.chunks_exact(MAP_RECORD_BYTES)
}
