use std::{env, f64::consts::PI, fs, path::PathBuf};

use mvt_reader::{Reader, feature::Value};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

const WIDTH: usize = 960;
const HEIGHT: usize = 680;
const PANEL_BYTES: usize = (WIDTH * HEIGHT) / 8;
// Each packed byte holds 8 horizontal pixels, so rows must not straddle bytes.
const _: () = assert!(WIDTH % 8 == 0);

/// OpenFreeMap: free OpenStreetMap vector tiles (OpenMapTiles schema), no API key.
/// Its terms ask offline downloads to identify themselves with a unique User-Agent.
const TILEJSON_URL: &str = "https://tiles.openfreemap.org/planet";
const USER_AGENT: &str = "epaper-map-builder/0.1";
const TILE_SIZE: u32 = 256;

const DEFAULT_ZOOMS: &str = "13";
const DEFAULT_LATITUDE: &str = "30.2672";
const DEFAULT_LONGITUDE: &str = "-97.7431";

/// The map uses the panel's two lightest gray levels; the darker gray and black
/// are left for overlays.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    White,
    LightGray,
}

impl Tone {
    /// The gray value each tone is rendered with before being mapped back to a `Tone`.
    const fn luma(self) -> u8 {
        match self {
            Tone::White => 255,
            Tone::LightGray => 200,
        }
    }

    fn from_luma(luma: u8) -> Self {
        [Tone::White, Tone::LightGray]
            .into_iter()
            .find(|tone| tone.luma() == luma)
            .unwrap_or_else(|| panic!("rendered unexpected gray value {luma}"))
    }

    /// The pixel's bits in the panel's two RAMs in 4-gray mode, per Waveshare's
    /// EPD_13in3k driver: white 0/0, light gray 1/0, gray 0/1, black 1/1.
    const fn ram_bits(self) -> (bool, bool) {
        match self {
            Tone::White => (false, false),
            Tone::LightGray => (true, false),
        }
    }
}

/// Filled areas, painted in order so later layers cover earlier ones.
/// `None` matches every feature in the layer.
const AREAS: &[(&str, Option<&[&str]>, Tone)] = &[
    ("landcover", Some(&["wood", "grass", "farmland", "wetland"]), Tone::LightGray),
    ("park", None, Tone::LightGray),
    ("water", None, Tone::LightGray),
    ("aeroway", Some(&["runway", "taxiway", "apron"]), Tone::LightGray),
];

/// Line width in pixels and tone by feature class; unlisted classes aren't drawn.
const WATERWAYS: &[(&str, f32, Tone)] = &[
    ("river", 2.0, Tone::LightGray),
    ("canal", 1.0, Tone::LightGray),
];
const ROADS: &[(&str, f32, Tone)] = &[
    ("motorway", 3.0, Tone::LightGray),
    ("trunk", 3.0, Tone::LightGray),
    ("primary", 3.0, Tone::LightGray),
    ("secondary", 2.0, Tone::LightGray),
    ("tertiary", 2.0, Tone::LightGray),
    ("minor", 1.0, Tone::LightGray),
    ("rail", 1.0, Tone::LightGray),
];

fn main() {
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MAP_ZOOMS");
    println!("cargo:rerun-if-env-changed=MAP_LATITUDE");
    println!("cargo:rerun-if-env-changed=MAP_LONGITUDE");

    let zooms = parse_zooms(&env::var("MAP_ZOOMS").unwrap_or_else(|_| DEFAULT_ZOOMS.into()));
    let latitude = parse_coordinate("MAP_LATITUDE", DEFAULT_LATITUDE, -85.0, 85.0);
    let longitude = parse_coordinate("MAP_LONGITUDE", DEFAULT_LONGITUDE, -180.0, 180.0);
    println!("cargo:rustc-env=MAP_LATITUDE={latitude}");
    println!("cargo:rustc-env=MAP_LONGITUDE={longitude}");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));

    let tile_url = tile_url_template();
    let mut maps = Vec::with_capacity(zooms.len() * (1 + 2 * PANEL_BYTES));
    for zoom in zooms {
        let map = render_map(&tile_url, zoom, latitude.into(), longitude.into());
        let (bw_ram, red_ram) = pack_ram_planes(&map);
        maps.push(zoom);
        maps.extend(bw_ram);
        maps.extend(red_ram);
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
        zooms.iter().all(|zoom| *zoom <= 14),
        "OpenFreeMap zoom levels must be from 0 to 14"
    );
    zooms.sort();
    assert!(
        zooms.windows(2).all(|pair| pair[0] != pair[1]),
        "MAP_ZOOMS cannot contain duplicates"
    );
    zooms
}

fn get(url: &str) -> Vec<u8> {
    ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .and_then(|mut response| response.body_mut().read_to_vec())
        .unwrap_or_else(|error| panic!("failed to download {url}: {error}"))
}

/// OpenFreeMap's tile URLs contain a dataset version that changes weekly, so read
/// the current template (`.../{z}/{x}/{y}.pbf`) from its TileJSON.
fn tile_url_template() -> String {
    let tilejson = String::from_utf8(get(TILEJSON_URL)).expect("TileJSON is UTF-8");
    let start = tilejson
        .find("https://")
        .expect("TileJSON contains a tile URL");
    let end = start + tilejson[start..].find('"').expect("tile URL is a JSON string");
    let template = &tilejson[start..end];
    assert!(
        template.contains("{z}/{x}/{y}"),
        "unexpected tile URL in TileJSON: {template}"
    );
    template.to_owned()
}

/// Web Mercator: converts a coordinate to its pixel position in the whole-world
/// image at `zoom`, which is `TILE_SIZE * 2^zoom` pixels on each side.
fn world_pixel(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    let world_size = f64::from(TILE_SIZE) * f64::from(1u32 << zoom);
    let x = (longitude + 180.0) / 360.0 * world_size;
    let y = (1.0 - latitude.to_radians().tan().asinh() / PI) / 2.0 * world_size;
    (x, y)
}

/// Downloads every vector tile overlapping a WIDTH x HEIGHT window centered on the
/// coordinate and draws them into one image of `Tone`s.
fn render_map(tile_url: &str, zoom: u8, latitude: f64, longitude: f64) -> Vec<Tone> {
    let (center_x, center_y) = world_pixel(latitude, longitude, zoom);
    let left = (center_x - WIDTH as f64 / 2.0).round() as i64;
    let top = (center_y - HEIGHT as f64 / 2.0).round() as i64;
    let tile = i64::from(TILE_SIZE);
    let tiles_per_side = 1i64 << zoom;

    let mut tiles = Vec::new();
    for tile_y in top.div_euclid(tile)..=(top + HEIGHT as i64 - 1).div_euclid(tile) {
        assert!(
            (0..tiles_per_side).contains(&tile_y),
            "map at zoom {zoom} extends past the top or bottom of the world"
        );
        for tile_x in left.div_euclid(tile)..=(left + WIDTH as i64 - 1).div_euclid(tile) {
            // Longitude wraps around, so tile columns do too.
            let wrapped_x = tile_x.rem_euclid(tiles_per_side);
            let url = tile_url
                .replace("{z}", &zoom.to_string())
                .replace("{x}", &wrapped_x.to_string())
                .replace("{y}", &tile_y.to_string());
            let reader = Reader::new(get(&url))
                .unwrap_or_else(|error| panic!("failed to parse tile {url}: {error:?}"));
            // Where this tile's top-left corner lands on the map, in pixels.
            let origin = ((tile_x * tile - left) as f32, (tile_y * tile - top) as f32);
            tiles.push((reader, origin));
        }
    }

    let mut pixmap = Pixmap::new(WIDTH as u32, HEIGHT as u32).expect("map size is valid");
    pixmap.fill(tiny_skia::Color::WHITE);

    // Draw layer by layer across all tiles, so e.g. water always covers parks.
    for &(layer, classes, tone) in AREAS {
        for (reader, origin) in &tiles {
            for (feature, scale) in features(reader, layer) {
                if classes.is_some_and(|classes| !classes.contains(&class(&feature))) {
                    continue;
                }
                fill_polygons(&mut pixmap, &feature.geometry, *origin, scale, tone);
            }
        }
    }
    for (layer, widths) in [("waterway", WATERWAYS), ("transportation", ROADS)] {
        for (reader, origin) in &tiles {
            for (feature, scale) in features(reader, layer) {
                let Some(&(_, width, tone)) = widths.iter().find(|(c, ..)| *c == class(&feature))
                else {
                    continue;
                };
                if property(&feature, "brunnel") == Some("tunnel") {
                    continue;
                }
                stroke_lines(&mut pixmap, &feature.geometry, *origin, scale, width, tone);
            }
        }
    }

    // Anti-aliasing is off, so every pixel is exactly one of the tones' gray values.
    pixmap
        .pixels()
        .iter()
        .map(|pixel| Tone::from_luma(pixel.red()))
        .collect()
}

/// The features of one layer, each with the scale from tile units to pixels.
fn features(reader: &Reader, layer: &str) -> Vec<(mvt_reader::feature::Feature, f32)> {
    let layers = reader.get_layer_metadata().expect("tile has layer metadata");
    let Some(meta) = layers.iter().find(|meta| meta.name == layer) else {
        return Vec::new();
    };
    let scale = TILE_SIZE as f32 / meta.extent as f32;
    reader
        .get_features(meta.layer_index)
        .expect("layer features decode")
        .into_iter()
        .map(|feature| (feature, scale))
        .collect()
}

fn property<'a>(feature: &'a mvt_reader::feature::Feature, key: &str) -> Option<&'a str> {
    match feature.properties.as_ref()?.get(key)? {
        Value::String(value) => Some(value),
        _ => None,
    }
}

fn class(feature: &mvt_reader::feature::Feature) -> &str {
    property(feature, "class").unwrap_or("")
}

fn paint(tone: Tone) -> Paint<'static> {
    let mut paint = Paint::default();
    let luma = tone.luma();
    paint.set_color_rgba8(luma, luma, luma, 255);
    // Exact tones only: anti-aliasing would create in-between grays.
    paint.anti_alias = false;
    paint
}

fn fill_polygons(
    pixmap: &mut Pixmap,
    geometry: &geo_types::Geometry<f32>,
    origin: (f32, f32),
    scale: f32,
    tone: Tone,
) {
    let polygons = match geometry {
        geo_types::Geometry::Polygon(polygon) => vec![polygon.clone()],
        geo_types::Geometry::MultiPolygon(multi) => multi.0.clone(),
        _ => return,
    };
    for polygon in polygons {
        // One path per polygon with even-odd filling, so its holes (islands in a
        // lake, clearings in a park) stay unpainted.
        let mut path = PathBuilder::new();
        for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
            add_points(&mut path, ring.0.iter(), origin, scale);
            path.close();
        }
        if let Some(path) = path.finish() {
            pixmap.fill_path(&path, &paint(tone), FillRule::EvenOdd, Transform::identity(), None);
        }
    }
}

fn stroke_lines(
    pixmap: &mut Pixmap,
    geometry: &geo_types::Geometry<f32>,
    origin: (f32, f32),
    scale: f32,
    width: f32,
    tone: Tone,
) {
    let lines = match geometry {
        geo_types::Geometry::LineString(line) => vec![line.clone()],
        geo_types::Geometry::MultiLineString(multi) => multi.0.clone(),
        _ => return,
    };
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for line in lines {
        let mut path = PathBuilder::new();
        add_points(&mut path, line.0.iter(), origin, scale);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &paint(tone), &stroke, Transform::identity(), None);
        }
    }
}

/// Adds tile-unit points to the path in map pixels. (Converting the points rather
/// than using a `Transform` keeps stroke widths in pixels.)
fn add_points<'a>(
    path: &mut PathBuilder,
    points: impl Iterator<Item = &'a geo_types::Coord<f32>>,
    (origin_x, origin_y): (f32, f32),
    scale: f32,
) {
    for (index, point) in points.enumerate() {
        let (x, y) = (origin_x + point.x * scale, origin_y + point.y * scale);
        if index == 0 {
            path.move_to(x, y);
        } else {
            path.line_to(x, y);
        }
    }
}

/// Splits the map into the two bit planes the panel's RAMs take in 4-gray mode
/// (command 0x24 "BW RAM", then 0x26 "RED RAM"), 8 pixels per byte, MSB first.
fn pack_ram_planes(map: &[Tone]) -> (Vec<u8>, Vec<u8>) {
    let pack = |bit: fn((bool, bool)) -> bool| -> Vec<u8> {
        map.chunks_exact(8)
            .map(|pixels| {
                pixels
                    .iter()
                    .fold(0, |byte, tone| (byte << 1) | u8::from(bit(tone.ram_bits())))
            })
            .collect()
    };
    (pack(|(bw, _)| bw), pack(|(_, red)| red))
}
