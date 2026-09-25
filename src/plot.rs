use plotters::prelude::RGBColor;
use std::fs;
use std::io;
use std::path::Path;

// Shared chart typography and layout requirements. All user-facing chart text
// uses Lilex Nerd Font; all colors come from the Flexoki palette.
pub(crate) const FONT_FAMILY: &str = "Lilex Nerd Font";
pub(crate) const CHART_WIDTH: u32 = 1400;
pub(crate) const CHART_HEIGHT: u32 = 800;
pub(crate) const CHART_TITLE_SIZE: u32 = 48;
pub(crate) const CHART_MARGIN: u32 = 36;
pub(crate) const LABEL_AREA_SIZE: u32 = 112;
pub(crate) const Y_LABEL_AREA_SIZE: u32 = 176;
pub(crate) const AXIS_DESC_FONT_SIZE: u32 = 40;
pub(crate) const TICK_LABEL_FONT_SIZE: u32 = 32;
pub(crate) const LEGEND_WIDTH: i32 = 20;
pub(crate) const LEGEND_FONT_SIZE: u32 = 18;

pub(crate) const PAPER: RGBColor = RGBColor(255, 252, 240);
pub(crate) const BASE_150: RGBColor = RGBColor(218, 216, 206);
pub(crate) const BASE_300: RGBColor = RGBColor(183, 181, 172);
pub(crate) const BASE_800: RGBColor = RGBColor(64, 62, 60);
pub(crate) const BLUE_600: RGBColor = RGBColor(32, 94, 166);
pub(crate) const CYAN_600: RGBColor = RGBColor(36, 131, 123);
pub(crate) const GREEN_600: RGBColor = RGBColor(102, 128, 11);
pub(crate) const YELLOW_600: RGBColor = RGBColor(173, 131, 1);
pub(crate) const RED_600: RGBColor = RGBColor(175, 48, 41);

pub(crate) fn create_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
