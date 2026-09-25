use super::PlayerSeason;
use plotters::coord::types::{RangedCoordf64, RangedCoordi64};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const FONT_FAMILY: &str = "Lilex Nerd Font";
const CHART_WIDTH: u32 = 1400;
const CHART_HEIGHT: u32 = 800;
const CHART_TITLE_SIZE: u32 = 48;
const CHART_MARGIN: u32 = 36;
const X_LABEL_AREA_SIZE: u32 = 96;
const Y_LABEL_AREA_SIZE: u32 = 176;
const AXIS_DESC_FONT_SIZE: u32 = 40;
const TICK_LABEL_FONT_SIZE: u32 = 32;
const LEGEND_WIDTH: i32 = 20;
const LEGEND_FONT_SIZE: u32 = 18;
const POINT_RADIUS: i32 = 2;
const ELO_BIN_SIZE: i64 = 50;
const WINRATE_BIN_SIZE: i64 = 100;
const MIN_WINRATE_BIN_PLAYERS: u64 = 20;
const LEGEND_POSITION: SeriesLabelPosition = SeriesLabelPosition::UpperRight;

const PAPER: RGBColor = RGBColor(255, 252, 240);
const BASE_150: RGBColor = RGBColor(218, 216, 206);
const BASE_300: RGBColor = RGBColor(183, 181, 172);
const BASE_800: RGBColor = RGBColor(64, 62, 60);
const BLUE_600: RGBColor = RGBColor(32, 94, 166);
const GREEN_600: RGBColor = RGBColor(102, 128, 11);

pub fn draw_elo_vs_games(
    path: &Path,
    season: u32,
    players: &[&PlayerSeason],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let points: Vec<_> = players
        .iter()
        .filter_map(|player| Some((i64::try_from(player.games_played).ok()?, player.latest_elo?)))
        .collect();
    let max_games = points
        .iter()
        .map(|(games, _)| *games)
        .max()
        .unwrap_or(1)
        .max(1);
    let (min_elo, max_elo) = elo_bounds(&points)?;
    let (min_elo, max_elo) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Latest Elo vs Games Played"),
            (FONT_FAMILY, CHART_TITLE_SIZE),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0i64..(max_games + max_games / 20 + 1), min_elo..max_elo)?;

    chart
        .configure_mesh()
        .x_desc("Games played in season")
        .y_desc("Latest Elo in season")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(8)
        .y_labels(8)
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(points.iter().map(|(games, elo)| {
            Circle::new((*games, *elo), POINT_RADIUS, BLUE_600.mix(0.35).filled())
        }))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    chart
        .configure_series_labels()
        .position(LEGEND_POSITION)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    root.present()?;
    Ok(())
}

pub fn draw_elo_distribution(
    path: &Path,
    season: u32,
    players: &[&PlayerSeason],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let mut bins = std::collections::BTreeMap::<i64, u64>::new();
    for elo in players.iter().filter_map(|player| player.latest_elo) {
        *bins
            .entry(elo.div_euclid(ELO_BIN_SIZE) * ELO_BIN_SIZE)
            .or_default() += 1;
    }
    if bins.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot").into());
    }
    let first_bin = *bins.first_key_value().expect("non-empty bins").0;
    let last_bin = *bins.last_key_value().expect("non-empty bins").0;
    let max_count = *bins.values().max().expect("non-empty bins");
    let x_end = last_bin + ELO_BIN_SIZE;

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Elo Distribution"),
            (FONT_FAMILY, CHART_TITLE_SIZE),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(first_bin..x_end, 0u64..(max_count + max_count / 10 + 1))?;

    chart
        .configure_mesh()
        .x_desc("Latest Elo in season (50 point bins)")
        .y_desc("Players")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(8)
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(bins.iter().map(|(bin, count)| {
            Rectangle::new(
                [(*bin, 0), (*bin + ELO_BIN_SIZE - 4, *count)],
                BLUE_600.mix(0.85).filled(),
            )
        }))?
        .label("Players")
        .legend(|(x, y)| {
            Rectangle::new(
                [(x, y - 5), (x + LEGEND_WIDTH, y + 5)],
                BLUE_600.mix(0.85).filled(),
            )
        });
    chart
        .configure_series_labels()
        .position(LEGEND_POSITION)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    root.present()?;
    Ok(())
}

pub fn draw_winrate_vs_elo(
    path: &Path,
    season: u32,
    players: &[&PlayerSeason],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let points: Vec<_> = players
        .iter()
        .filter_map(|player| Some((player.latest_elo?, player.win_rate()?)))
        .collect();
    if points.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no players with Elo and decided matches to plot",
        )
        .into());
    }

    let min_elo = points.iter().map(|(elo, _)| *elo).min().expect("non-empty");
    let max_elo = points.iter().map(|(elo, _)| *elo).max().expect("non-empty");
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);
    let mut bins = std::collections::BTreeMap::<i64, (f64, u64)>::new();
    for (elo, win_rate) in &points {
        let bin = elo.div_euclid(WINRATE_BIN_SIZE) * WINRATE_BIN_SIZE;
        let (sum, count) = bins.entry(bin).or_default();
        *sum += win_rate;
        *count += 1;
    }
    let mean_by_elo: Vec<_> = bins
        .into_iter()
        .filter_map(|(bin, (sum, count))| {
            (count >= MIN_WINRATE_BIN_PLAYERS)
                .then_some((bin + WINRATE_BIN_SIZE / 2, sum / count as f64))
        })
        .collect();

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Player Winrate vs Elo"),
            (FONT_FAMILY, CHART_TITLE_SIZE),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(x_min..x_max, 0.0f64..1.0f64)?;

    chart
        .configure_mesh()
        .x_desc("Latest Elo in season")
        .y_desc("Win rate (decided matches)")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(6)
        .y_label_formatter(&|rate| format!("{:.0}%", rate * 100.0))
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart.draw_series(LineSeries::new(
        [(x_min, 0.5), (x_max, 0.5)],
        BASE_300.stroke_width(2),
    ))?;
    chart
        .draw_series(points.iter().map(|(elo, win_rate)| {
            Circle::new((*elo, *win_rate), POINT_RADIUS, BLUE_600.mix(0.25).filled())
        }))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    chart
        .draw_series(LineSeries::new(
            mean_by_elo.iter().copied(),
            GREEN_600.stroke_width(3),
        ))?
        .label("Mean by Elo (20+ players per bin)")
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + LEGEND_WIDTH, y)],
                GREEN_600.stroke_width(3),
            )
        });
    chart.draw_series(
        mean_by_elo
            .iter()
            .map(|(elo, win_rate)| Circle::new((*elo, *win_rate), 4, GREEN_600.filled())),
    )?;
    chart
        .configure_series_labels()
        .position(LEGEND_POSITION)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    root.present()?;
    Ok(())
}

pub fn draw_winrate_over_time(
    path: &Path,
    season: u32,
    points: &[(i64, f64, f64)],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let min_elo = points
        .iter()
        .map(|(elo, _, _)| *elo)
        .min()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no temporal Elo values"))?;
    let max_elo = points
        .iter()
        .map(|(elo, _, _)| *elo)
        .max()
        .expect("non-empty");
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Winrate vs Elo Over Time"),
            (FONT_FAMILY, CHART_TITLE_SIZE),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(x_min..x_max, 0.0f64..1.0f64)?;

    chart
        .configure_mesh()
        .x_desc("Elo at match time")
        .y_desc("Cumulative win rate at match time")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(6)
        .y_label_formatter(&|rate| format!("{:.0}%", rate * 100.0))
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart.draw_series(points.iter().map(|(elo, win_rate, progress)| {
        Circle::new(
            (*elo, *win_rate),
            POINT_RADIUS,
            progress_color(*progress).mix(0.50).filled(),
        )
    }))?;
    draw_progress_legend(&chart, x_min, x_max)?;
    root.present()?;
    Ok(())
}

fn draw_progress_legend<DB: DrawingBackend>(
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
        (FONT_FAMILY, 18).into_font().color(&BASE_800),
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
        (right - margin * 5, 0.91),
        (FONT_FAMILY, 16).into_font().color(&BASE_800),
    ))?;
    Ok(())
}

fn progress_color(progress: f64) -> RGBColor {
    const STOPS: [RGBColor; 5] = [
        RGBColor(32, 94, 166),  // Flexoki blue 600
        RGBColor(36, 131, 123), // Flexoki cyan 600
        RGBColor(102, 128, 11), // Flexoki green 600
        RGBColor(173, 131, 1),  // Flexoki yellow 600
        RGBColor(175, 48, 41),  // Flexoki red 600
    ];
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

fn elo_bounds(points: &[(i64, i64)]) -> Result<(i64, i64), Box<dyn Error>> {
    let min = points
        .iter()
        .map(|(_, elo)| *elo)
        .min()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot"))?;
    let max = points.iter().map(|(_, elo)| *elo).max().expect("non-empty");
    Ok((min, max))
}

fn padded_bounds(min: i64, max: i64) -> (i64, i64) {
    let span = (max - min).max(1);
    let padding = (span / 20).max(25);
    (min.saturating_sub(padding), max.saturating_add(padding))
}

fn create_parent(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
