//! Original first-run artwork: an OFL pixel font and generated geometric widgets.
//!
//! This never reads Minecraft resources. Rasterizing the pixel font once at an
//! integer GUI size removes the grayscale / half-pixel advances of the previous
//! oversampled Geist provider without changing the renderer used by the game.
use ab_glyph::Font;
use std::{
    fs,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CELL: u32 = 12;
const COLUMNS: usize = 16;
const BASELINE: f32 = 9.0;
const PIXELIFY: &[u8] = include_bytes!("../../../horizon-theme/fonts/PixelifySans-Medium.ttf");

/// Build only freely redistributable assets for the account / download screen.
pub(super) fn original_artwork(internal: &Path) -> Result<PathBuf> {
    let root = internal.join("bootstrap-ui");
    let fonts = root.join("assets/minecraft/font");
    let textures = root.join("assets/minecraft/textures/font");
    fs::create_dir_all(&fonts)?;
    fs::create_dir_all(&textures)?;
    let font = ab_glyph::FontRef::try_from_slice(PIXELIFY)?;
    let mut chars: Vec<char> = font
        .codepoint_ids()
        .filter_map(|(id, ch)| (id.0 != 0 && !ch.is_control() && !ch.is_whitespace()).then_some(ch))
        .collect();
    chars.sort_unstable();
    chars.dedup();
    let rows = chars.len().div_ceil(COLUMNS);
    let width = CELL * COLUMNS as u32;
    let height = CELL * rows as u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    // Pixelify uses a 1000-unit em. Ten pixels per em gives its seven-pixel
    // capitals and room for accented Latin letters and descenders in this cell.
    let scale = ab_glyph::PxScale::from(
        10.0 * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0),
    );
    for (index, ch) in chars.iter().enumerate() {
        let glyph = font
            .glyph_id(*ch)
            .with_scale_and_position(scale, ab_glyph::point(0.0, BASELINE));
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            let cx = (index % COLUMNS) as u32 * CELL;
            let cy = (index / COLUMNS) as u32 * CELL;
            outline.draw(|x, y, coverage| {
                let x = bounds.min.x as i32 + x as i32;
                let y = bounds.min.y as i32 + y as i32;
                // A binary bitmap is deliberate: whole GUI pixels remain crisp
                // when enlarged by the HUD's existing nearest-neighbor sampler.
                if coverage >= 0.5 && (0..CELL as i32).contains(&x) && (0..CELL as i32).contains(&y)
                {
                    let offset = (((cy + y as u32) * width + cx + x as u32) * 4) as usize;
                    rgba[offset..offset + 4].fill(255);
                }
            });
        }
    }
    write_png(&textures.join("bootstrap_pixel.png"), width, height, &rgba)?;
    let mut grid = Vec::with_capacity(rows);
    for chunk in chars.chunks(COLUMNS) {
        let mut row: String = chunk.iter().collect();
        row.extend(std::iter::repeat_n('\0', COLUMNS - chunk.len()));
        grid.push(row);
    }
    fs::write(fonts.join("geist.ttf"), horizon_theme::fonts::GEIST_REGULAR)?;
    let skip: String = chars.iter().collect();
    let definition = serde_json::json!({"providers":[
        {"type":"space","advances":{" ":4}},
        {"type":"bitmap","file":"minecraft:font/bootstrap_pixel.png","height":CELL,"ascent":BASELINE,"chars":grid},
        {"type":"ttf","file":"minecraft:geist.ttf","size":9,"oversample":1,"shift":[0,0],"skip":skip}
    ]});
    fs::write(fonts.join("default.json"), serde_json::to_vec(&definition)?)?;
    let widgets = root.join("assets/minecraft/textures/gui/sprites/widget");
    fs::create_dir_all(&widgets)?;
    for (name, fill, border) in [
        ("button", [55, 58, 77, 255], [94, 99, 124, 255]),
        (
            "button_highlighted",
            [105, 88, 164, 255],
            [185, 161, 244, 255],
        ),
        ("button_disabled", [36, 38, 49, 255], [56, 59, 72, 255]),
    ] {
        let mut pixels = fill.repeat(20 * 20);
        for y in 0..20usize {
            for x in 0..20usize {
                if x == 0 || y == 0 || x == 19 || y == 19 {
                    pixels[(y * 20 + x) * 4..(y * 20 + x + 1) * 4].copy_from_slice(&border);
                }
            }
        }
        write_png(&widgets.join(format!("{name}.png")), 20, 20, &pixels)?;
    }
    fs::write(
        root.join("OFL-PixelifySans.txt"),
        include_bytes!("../../../horizon-theme/fonts/OFL-PixelifySans.txt"),
    )?;
    fs::write(
        root.join("OFL-Geist.txt"),
        include_bytes!("../../../horizon-theme/fonts/OFL-Geist.txt"),
    )?;
    Ok(root)
}
fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let mut encoder = png::Encoder::new(fs::File::create(path)?, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}
