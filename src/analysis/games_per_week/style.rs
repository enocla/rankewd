use plotters::prelude::{RGBColor, SeriesLabelPosition};

pub const OUTPUT_FILE: &str = "games_per_week.png";
pub const FONT_FAMILY: &str = "Lilex Nerd Font";
pub const CHART_TITLE: &str = "Games Played per Week";
pub const X_AXIS_TITLE: &str = "UTC week (ISO)";
pub const Y_AXIS_TITLE: &str = "Games played";

pub const CHART_WIDTH: u32 = 1400;
pub const CHART_HEIGHT: u32 = 800;
pub const CHART_TITLE_SIZE: u32 = 48;
pub const CHART_MARGIN: u32 = 36;
pub const X_LABEL_AREA_SIZE: u32 = 112;
pub const Y_LABEL_AREA_SIZE: u32 = 160;
pub const AXIS_DESC_FONT_SIZE: u32 = 40;
pub const TICK_LABEL_FONT_SIZE: u32 = 32;
pub const MAX_X_LABELS: usize = 12;
pub const DATA_POINT_RADIUS: i32 = 4;
pub const LEGEND_WIDTH: i32 = 20;
pub const LEGEND_FONT_SIZE: u32 = 18;
pub const LEGEND_POSITION: SeriesLabelPosition = SeriesLabelPosition::UpperRight;

// Flexoki palette values from https://stephango.com/flexoki.
pub const PAPER: RGBColor = RGBColor(255, 252, 240);
pub const BASE_150: RGBColor = RGBColor(218, 216, 206);
pub const BASE_300: RGBColor = RGBColor(183, 181, 172);
pub const BASE_800: RGBColor = RGBColor(64, 62, 60);
pub const BLUE_600: RGBColor = RGBColor(32, 94, 166);
