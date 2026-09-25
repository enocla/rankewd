use plotters::coord::types::{RangedCoordf64, RangedCoordi64};
use plotters::prelude::*;
use std::error::Error;
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
pub(crate) const POINT_RADIUS: i32 = 2;

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

/// Expands an axis range so data points do not touch the frame.
pub(crate) fn padded_bounds(min: i64, max: i64) -> (i64, i64) {
    let span = max.saturating_sub(min).max(1);
    let padding = (span / 20).max(25);
    (min.saturating_sub(padding), max.saturating_add(padding))
}

/// Pearson correlation of the x and y series, or `None` when the two series
/// cannot vary relative to each other.
pub(crate) fn pearson_r(points: &[(i64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let count = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| *x as f64).sum::<f64>() / count;
    let mean_y = points.iter().map(|(_, y)| *y).sum::<f64>() / count;

    let mut covariance = 0.0;
    let mut variance_x = 0.0;
    let mut variance_y = 0.0;
    for (x, y) in points {
        let offset_x = (*x as f64) - mean_x;
        let offset_y = *y - mean_y;
        covariance += offset_x * offset_y;
        variance_x += offset_x * offset_x;
        variance_y += offset_y * offset_y;
    }

    let denominator = (variance_x * variance_y).sqrt();
    (denominator > 0.0).then(|| (covariance / denominator).clamp(-1.0, 1.0))
}

/// Reports an R value for the CLI report, naming the reason it is missing.
pub(crate) fn format_correlation(correlation: Option<f64>) -> String {
    match correlation {
        Some(value) => format!("{value:.4}"),
        None => "undefined (neither series varies)".to_owned(),
    }
}

/// Ticks to show on a log-scaled axis spanning `y_min` to `y_max`.
pub(crate) fn log_key_points(y_min: f64, y_max: f64) -> Vec<f64> {
    let min_exponent = y_min.log10().floor() as i32;
    let max_exponent = y_max.log10().ceil() as i32;
    let mut points = Vec::new();
    for exponent in min_exponent - 1..=max_exponent {
        for multiplier in [1.0, 2.0, 5.0] {
            let point = multiplier * 10f64.powi(exponent);
            if point >= y_min && point <= y_max {
                points.push(point);
            }
        }
    }
    if points.is_empty() {
        points.push(y_min);
    }
    points
}

/// Draws the upper-right legend used by series that carry labels.
pub(crate) fn draw_series_legend<'a, DB, X, Y>(
    chart: &mut ChartContext<'a, DB, Cartesian2d<X, Y>>,
) -> Result<(), Box<dyn Error>>
where
    DB: DrawingBackend + 'a,
    DB::ErrorType: 'static,
    X: Ranged,
    Y: Ranged,
    Cartesian2d<X, Y>: CoordTranslate,
{
    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE, &BASE_800))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    Ok(())
}

/// Flexoki gradient color for normalized progress through a season.
pub(crate) fn progress_color(progress: f64) -> RGBColor {
    const STOPS: [RGBColor; 5] = [BLUE_600, CYAN_600, GREEN_600, YELLOW_600, RED_600];
    let position = progress.clamp(0.0, 1.0) * (STOPS.len() - 1) as f64;
    let start_index = (position.floor() as usize).min(STOPS.len() - 2);
    let fraction = position - start_index as f64;
    let start = STOPS[start_index];
    let end = STOPS[start_index + 1];
    let interpolate = |left: u8, right: u8| {
        (f64::from(left) + (f64::from(right) - f64::from(left)) * fraction).round() as u8
    };
    RGBColor(
        interpolate(start.0, end.0),
        interpolate(start.1, end.1),
        interpolate(start.2, end.2),
    )
}

/// Draws the season-progress color bar in the upper-right of a chart.
pub(crate) fn draw_progress_legend<DB: DrawingBackend>(
    chart: &ChartContext<'_, DB, Cartesian2d<RangedCoordi64, RangedCoordf64>>,
    x_min: i64,
    x_max: i64,
) -> Result<(), Box<dyn Error>>
where
    DB::ErrorType: 'static,
{
    let span = x_max.saturating_sub(x_min).max(1);
    let margin = (span / 50).max(1);
    let right = x_max.saturating_sub(margin);
    let left = right.saturating_sub((span / 3).max(1)).max(x_min);
    let bottom = 0.895;
    let top = 0.995;
    let bar_bottom = 0.935;
    let bar_top = 0.96;
    let area = chart.plotting_area();

    area.draw(&Rectangle::new(
        [(left, bottom), (right, top)],
        PAPER.filled().stroke_width(1),
    ))?;
    area.draw(&Text::new(
        "Season progress",
        (left + margin, 0.979),
        (FONT_FAMILY, LEGEND_FONT_SIZE).into_font().color(&BASE_800),
    ))?;

    const SEGMENTS: i64 = 160;
    for segment in 0..SEGMENTS {
        let segment_left = left + (right - left) * segment / SEGMENTS;
        let segment_right = left + (right - left) * (segment + 1) / SEGMENTS;
        area.draw(&Rectangle::new(
            [(segment_left, bar_bottom), (segment_right, bar_top)],
            progress_color(segment as f64 / (SEGMENTS - 1) as f64).filled(),
        ))?;
    }
    area.draw(&Rectangle::new(
        [(left, bar_bottom), (right, bar_top)],
        BASE_800.stroke_width(1),
    ))?;
    area.draw(&Text::new(
        "Start",
        (left + margin, 0.91),
        (FONT_FAMILY, 16).into_font().color(&BASE_800),
    ))?;
    area.draw(&Text::new(
        "End",
        (right.saturating_sub(margin * 5), 0.91),
        (FONT_FAMILY, 16).into_font().color(&BASE_800),
    ))?;
    Ok(())
}
