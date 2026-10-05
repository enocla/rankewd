use plotters::chart::MeshStyle;
use plotters::coord::Shift;
use plotters::coord::types::{RangedCoordf64, RangedCoordi64};
use plotters::prelude::*;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

// Shared chart typography and layout requirements. All user-facing chart text
// uses SF Pro Text; all colors come from the Flexoki palette.
pub(crate) const FONT_FAMILY: &str = "SF Pro Text";
pub(crate) const CHART_WIDTH: u32 = 2800;
pub(crate) const CHART_HEIGHT: u32 = 1600;
pub(crate) const CHART_TITLE_SIZE: u32 = 96;
pub(crate) const CHART_MARGIN: u32 = 72;
pub(crate) const LABEL_AREA_SIZE: u32 = 224;
pub(crate) const Y_LABEL_AREA_SIZE: u32 = 352;
pub(crate) const AXIS_DESC_FONT_SIZE: u32 = 80;
pub(crate) const TICK_LABEL_FONT_SIZE: u32 = 64;
pub(crate) const LEGEND_WIDTH: i32 = 40;
pub(crate) const LEGEND_FONT_SIZE: u32 = 36;
pub(crate) const POINT_RADIUS: i32 = 4;

pub(crate) const PAPER: RGBColor = RGBColor(255, 252, 240);
pub(crate) const BASE_150: RGBColor = RGBColor(218, 216, 206);
pub(crate) const BASE_300: RGBColor = RGBColor(183, 181, 172);
pub(crate) const BASE_800: RGBColor = RGBColor(64, 62, 60);
pub(crate) const BLUE_600: RGBColor = RGBColor(32, 94, 166);
pub(crate) const CYAN_600: RGBColor = RGBColor(36, 131, 123);
pub(crate) const GREEN_600: RGBColor = RGBColor(102, 128, 11);
pub(crate) const YELLOW_600: RGBColor = RGBColor(173, 131, 1);
pub(crate) const RED_600: RGBColor = RGBColor(175, 48, 41);
pub(crate) const ORANGE_600: RGBColor = RGBColor(188, 82, 21);
pub(crate) const PURPLE_600: RGBColor = RGBColor(94, 64, 157);
pub(crate) const MAGENTA_600: RGBColor = RGBColor(160, 47, 111);
pub(crate) const BASE_500: RGBColor = RGBColor(135, 133, 128);

/// Distinct Flexoki accents for categorical series, in legend order.
pub(crate) const SERIES_COLORS: [RGBColor; 8] = [
    BLUE_600,
    RED_600,
    GREEN_600,
    ORANGE_600,
    PURPLE_600,
    CYAN_600,
    MAGENTA_600,
    YELLOW_600,
];

/// Line width of the data series in the added line charts.
pub(crate) const SERIES_STROKE: u32 = 6;
/// Font size of the values printed inside heatmap cells.
pub(crate) const CELL_FONT_SIZE: u32 = 40;

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
        .legend_area_size(60)
        .margin(20)
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
        (FONT_FAMILY, 32).into_font().color(&BASE_800),
    ))?;
    area.draw(&Text::new(
        "End",
        (right.saturating_sub(margin * 5), 0.91),
        (FONT_FAMILY, 32).into_font().color(&BASE_800),
    ))?;
    Ok(())
}

/// Creates the parent directory, then a paper-filled 2800×1600 canvas at `path`.
pub(crate) fn canvas(path: &Path) -> Result<DrawingArea<BitMapBackend<'_>, Shift>, Box<dyn Error>> {
    create_parent(path)?;
    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    Ok(root)
}

/// A chart builder with the shared title, margin, and label areas applied.
pub(crate) fn chart_builder<'a, 'b, DB: DrawingBackend>(
    root: &'a DrawingArea<DB, Shift>,
    title: &str,
) -> ChartBuilder<'a, 'b, DB> {
    let mut builder = ChartBuilder::on(root);
    builder
        .caption(title, (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800))
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE);
    builder
}

/// Applies the shared axis typography, colors, and grid to a mesh.
pub(crate) fn style_mesh<'m, 'a, 'b, X, Y, DB>(
    mesh: &'m mut MeshStyle<'a, 'b, X, Y, DB>,
    x_desc: &str,
    y_desc: &str,
) -> &'m mut MeshStyle<'a, 'b, X, Y, DB>
where
    X: Ranged,
    Y: Ranged,
    DB: DrawingBackend,
{
    mesh.x_desc(x_desc)
        .y_desc(y_desc)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .max_light_lines(2)
}

/// Legend glyph for a line series.
pub(crate) fn legend_line(color: RGBColor) -> impl Fn((i32, i32)) -> PathElement<(i32, i32)> {
    move |(x, y)| {
        PathElement::new(
            vec![(x, y), (x + LEGEND_WIDTH, y)],
            color.stroke_width(SERIES_STROKE),
        )
    }
}

/// Legend glyph for a filled bar or area.
pub(crate) fn legend_box(color: RGBColor) -> impl Fn((i32, i32)) -> Rectangle<(i32, i32)> {
    move |(x, y)| Rectangle::new([(x, y - 12), (x + LEGEND_WIDTH, y + 12)], color.filled())
}

/// Blends two colors; `fraction = 0` gives `from`, `1` gives `to`.
pub(crate) fn blend(from: RGBColor, to: RGBColor, fraction: f64) -> RGBColor {
    let fraction = fraction.clamp(0.0, 1.0);
    let channel = |left: u8, right: u8| {
        (f64::from(left) + (f64::from(right) - f64::from(left)) * fraction).round() as u8
    };
    RGBColor(
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

/// Sequential heatmap color from paper through cyan to deep blue.
pub(crate) fn heat_color(fraction: f64) -> RGBColor {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction < 0.5 {
        blend(PAPER, CYAN_600, fraction * 2.0)
    } else {
        blend(CYAN_600, BLUE_600, (fraction - 0.5) * 2.0)
    }
}

/// Diverging color: red below zero, paper at zero, blue above, saturating at
/// `limit` in either direction.
pub(crate) fn diverging_color(value: f64, limit: f64) -> RGBColor {
    let fraction = (value / limit.max(f64::EPSILON)).clamp(-1.0, 1.0);
    if fraction < 0.0 {
        blend(PAPER, RED_600, -fraction)
    } else {
        blend(PAPER, BLUE_600, fraction)
    }
}

/// Text color that stays legible on a cell of the given fill.
pub(crate) fn text_on(fill: RGBColor) -> RGBColor {
    let luminance =
        0.299 * f64::from(fill.0) + 0.587 * f64::from(fill.1) + 0.114 * f64::from(fill.2);
    if luminance < 140.0 { PAPER } else { BASE_800 }
}

/// Formats a match-clock duration in milliseconds as `m:ss`.
pub(crate) fn format_clock(milliseconds: f64) -> String {
    let seconds = (milliseconds / 1000.0).round().max(0.0) as i64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Names the selected seasons for a chart title, e.g. `Seasons 1–12`.
pub(crate) fn season_span(seasons: &[u32]) -> String {
    match (seasons.first(), seasons.last()) {
        (Some(first), Some(last)) if first == last => format!("Season {first}"),
        (Some(first), Some(last)) => format!("Seasons {first}–{last}"),
        _ => "No seasons".to_owned(),
    }
}

/// Tick label for a categorical season axis whose positions index `seasons`.
pub(crate) fn season_tick(seasons: &[u32], position: f64) -> String {
    let rounded = position.round();
    if (position - rounded).abs() > 1e-6 || rounded < 0.0 {
        return String::new();
    }
    seasons
        .get(rounded as usize)
        .map(|season| format!("S{season}"))
        .unwrap_or_default()
}

/// One labelled line of a [`LineChart`].
pub(crate) struct Line {
    pub(crate) label: String,
    pub(crate) color: RGBColor,
    pub(crate) points: Vec<(f64, f64)>,
    /// Whether each point also gets a marker.
    pub(crate) markers: bool,
}

impl Line {
    pub(crate) fn new(label: impl Into<String>, color: RGBColor, points: Vec<(f64, f64)>) -> Self {
        Self {
            label: label.into(),
            color,
            points,
            markers: true,
        }
    }
}

/// A line chart on linear axes, shared by the plots that compare a handful
/// of labelled series.
pub(crate) struct LineChart<'a> {
    pub(crate) title: String,
    pub(crate) x_desc: &'a str,
    pub(crate) y_desc: &'a str,
    pub(crate) x_range: std::ops::Range<f64>,
    pub(crate) y_range: std::ops::Range<f64>,
    pub(crate) x_labels: usize,
    pub(crate) x_format: &'a dyn Fn(&f64) -> String,
    pub(crate) y_format: &'a dyn Fn(&f64) -> String,
    pub(crate) lines: Vec<Line>,
    /// Dashed horizontal reference drawn beneath the lines, with its label.
    pub(crate) reference: Option<(f64, &'a str)>,
}

/// Renders a [`LineChart`] to `path`.
pub(crate) fn draw_line_chart(path: &Path, spec: &LineChart<'_>) -> Result<(), Box<dyn Error>> {
    let root = canvas(path)?;
    let mut chart = chart_builder(&root, &spec.title)
        .build_cartesian_2d(spec.x_range.clone(), spec.y_range.clone())?;
    style_mesh(&mut chart.configure_mesh(), spec.x_desc, spec.y_desc)
        .x_labels(spec.x_labels)
        .y_labels(8)
        .x_label_formatter(spec.x_format)
        .y_label_formatter(spec.y_format)
        .draw()?;

    if let Some((value, label)) = spec.reference {
        chart
            .draw_series(DashedLineSeries::new(
                [(spec.x_range.start, value), (spec.x_range.end, value)],
                24,
                16,
                BASE_500.stroke_width(4),
            ))?
            .label(label)
            .legend(legend_line(BASE_500));
    }
    for line in &spec.lines {
        chart
            .draw_series(LineSeries::new(
                line.points.iter().copied(),
                line.color.stroke_width(SERIES_STROKE),
            ))?
            .label(line.label.as_str())
            .legend(legend_line(line.color));
        if line.markers {
            chart.draw_series(
                line.points
                    .iter()
                    .map(|point| Circle::new(*point, POINT_RADIUS + 2, line.color.filled())),
            )?;
        }
    }
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}

/// Range from the smallest to the largest value, widened by `pad` of its span
/// below and `headroom` of its span above, so data clears the upper-right
/// legend.
pub(crate) fn value_range(
    values: impl IntoIterator<Item = f64>,
    pad: f64,
    headroom: f64,
) -> std::ops::Range<f64> {
    let (min, max) = values
        .into_iter()
        .filter(|value| value.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
            (min.min(value), max.max(value))
        });
    if !min.is_finite() {
        return 0.0..1.0;
    }
    let span = (max - min).max(f64::EPSILON);
    (min - span * pad)..(max + span * headroom)
}

/// Tick label for a percentage stored as a fraction.
pub(crate) fn percent_tick(value: &f64) -> String {
    format!("{:.0}%", value * 100.0)
}

/// Tick label for a whole number.
pub(crate) fn integer_tick(value: &f64) -> String {
    format!("{value:.0}")
}

/// A reusable chart component for uncertainty-bearing research series.
pub(crate) struct ResearchCurve {
    pub(crate) label: String,
    pub(crate) color: RGBColor,
    pub(crate) points: Vec<crate::dataset::ResearchEstimate>,
}
pub(crate) struct ResearchChart<'a> {
    pub(crate) title: &'a str,
    pub(crate) x_desc: &'a str,
    pub(crate) y_desc: &'a str,
    pub(crate) x_range: std::ops::Range<f64>,
    pub(crate) y_range: std::ops::Range<f64>,
    pub(crate) x_format: &'a dyn Fn(&f64) -> String,
    pub(crate) y_format: &'a dyn Fn(&f64) -> String,
    pub(crate) x_labels: usize,
    pub(crate) curves: Vec<ResearchCurve>,
    pub(crate) reference: Option<(f64, &'a str)>,
}
pub(crate) fn draw_research_chart(
    path: &Path,
    spec: &ResearchChart<'_>,
) -> Result<(), Box<dyn Error>> {
    let root = canvas(path)?;
    if spec.curves.iter().all(|c| c.points.is_empty()) {
        root.draw(&Text::new(
            spec.title,
            (140, 160),
            (FONT_FAMILY, CHART_TITLE_SIZE).into_font().color(&BASE_800),
        ))?;
        root.draw(&Text::new(
            "Insufficient data for the plotted estimates",
            (400, 800),
            (FONT_FAMILY, TICK_LABEL_FONT_SIZE)
                .into_font()
                .color(&BASE_800),
        ))?;
        root.present()?;
        return Ok(());
    }
    let mut chart = chart_builder(&root, spec.title)
        .build_cartesian_2d(spec.x_range.clone(), spec.y_range.clone())?;
    style_mesh(&mut chart.configure_mesh(), spec.x_desc, spec.y_desc)
        .x_labels(spec.x_labels)
        .y_labels(7)
        .x_label_formatter(spec.x_format)
        .y_label_formatter(spec.y_format)
        .draw()?;
    if let Some((y, label)) = spec.reference {
        chart
            .draw_series(DashedLineSeries::new(
                [(spec.x_range.start, y), (spec.x_range.end, y)],
                24,
                16,
                BASE_500.stroke_width(4),
            ))?
            .label(label)
            .legend(legend_line(BASE_500));
    }
    for curve in &spec.curves {
        if curve.points.is_empty() {
            continue;
        }
        chart
            .draw_series(LineSeries::new(
                curve.points.iter().map(|p| (p.x, p.mean)),
                curve.color.stroke_width(SERIES_STROKE),
            ))?
            .label(&curve.label)
            .legend(legend_line(curve.color));
        chart.draw_series(curve.points.iter().map(|p| {
            PathElement::new(
                vec![(p.x, p.low), (p.x, p.high)],
                curve.color.stroke_width(4),
            )
        }))?;
        chart.draw_series(
            curve
                .points
                .iter()
                .map(|p| Circle::new((p.x, p.mean), 8, curve.color.filled())),
        )?;
    }
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
