//! Palette and type scale, carried over from the Python prototype.

use egui::Color32;

pub const BG: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const PANEL: Color32 = Color32::from_rgb(0xfc, 0xfd, 0xfe);
pub const FRAME: Color32 = Color32::from_rgb(0xd7, 0xde, 0xe7);
pub const GRID: Color32 = Color32::from_rgb(0xea, 0xef, 0xf5);
pub const ZERO: Color32 = Color32::from_rgb(0xd3, 0xdc, 0xe6);
pub const TEXT: Color32 = Color32::from_rgb(0x3d, 0x4a, 0x5c);
pub const MUTED: Color32 = Color32::from_rgb(0x8d, 0x9a, 0xab);
pub const FAINT: Color32 = Color32::from_rgb(0xb6, 0xc0, 0xcd);
pub const DANGER: Color32 = Color32::from_rgb(0xd1, 0x49, 0x5b);
pub const WARN: Color32 = Color32::from_rgb(0xb4, 0x69, 0x0e);

pub const SERIES: [Color32; 8] = [
    Color32::from_rgb(0x2f, 0x6f, 0xd0),
    Color32::from_rgb(0xd1, 0x49, 0x5b),
    Color32::from_rgb(0x12, 0x87, 0x6f),
    Color32::from_rgb(0xc0, 0x7a, 0x12),
    Color32::from_rgb(0x7b, 0x4f, 0xc4),
    Color32::from_rgb(0x0d, 0x7f, 0x9e),
    Color32::from_rgb(0xc4, 0x4b, 0x90),
    Color32::from_rgb(0x5d, 0x8a, 0x1f),
];

pub fn series(index: usize) -> Color32 {
    SERIES[index % SERIES.len()]
}

/// Blend towards white, for fills that sit behind a trace.
pub fn tint(color: Color32, amount: f32) -> Color32 {
    let mix = |v: u8| (v as f32 + (255.0 - v as f32) * amount) as u8;
    Color32::from_rgb(mix(color.r()), mix(color.g()), mix(color.b()))
}

pub const TRACE_WIDTH: f32 = 1.4;
pub const THIN: f32 = 1.0;

pub const SIZE_TITLE: f32 = 19.0;
pub const SIZE_HEAD: f32 = 13.0;
pub const SIZE_BODY: f32 = 13.0;
pub const SIZE_SMALL: f32 = 11.5;
