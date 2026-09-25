use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{
    MIN_GAMES_FOR_WINRATE_PLOTS, PlayerSeasons, SampledTemporalPoints, WINRATE_POINTS_PER_PLAYER,
    eligible_winrate_uuids, elo_range,
};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, PAPER, TICK_LABEL_FONT_SIZE,
    Y_LABEL_AREA_SIZE, create_parent, draw_progress_legend, padded_bounds, progress_color,
};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "sampled_winrate_over_time_season_";
const MARKER_RADIUS: i32 = 4;
const X_AXIS_TITLE: &str = "Elo at sampled match";
const Y_AXIS_TITLE: &str = "Cumulative win rate at sampled match";

pub(super) struct SampledWinrateOverTime;

impl Analysis for SampledWinrateOverTime {
    fn name(&self) -> &'static str {
        "sampled-winrate-over-time"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;
        let seasons = data.seasons()?;

        let mut report = AnalysisReport::default();
        for season in seasons {
            let uuids = eligible_winrate_uuids(&season.players);
            if uuids.is_empty() {
                report.line(format!(
                    "Season {}: no players with at least {MIN_GAMES_FOR_WINRATE_PLOTS} matches",
                    season.season
                ));
                continue;
            }
            let sampled = season.history.sample_points(&uuids, season.season)?;
            if sampled.player_count == 0 {
                report.line(format!(
                    "Season {}: no players with {WINRATE_POINTS_PER_PLAYER} rated decided matches",
                    season.season
                ));
                continue;
            }
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &sampled)?;
            report.line(format!(
                "Season {}: {} players, {WINRATE_POINTS_PER_PLAYER} matches each",
                season.season, sampled.player_count
            ));
            report.line(format!("  Sampled win rate over time: {}", path.display()));
        }
        Ok(report)
    }
}

fn draw_chart(
    path: &Path,
    season: u32,
    sampled: &SampledTemporalPoints,
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let expected_points = sampled
        .player_count
        .checked_mul(WINRATE_POINTS_PER_PLAYER)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "too many sampled points"))?;
    if sampled.points.len() != expected_points {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "sampled player history does not contain five points per player",
        )
        .into());
    }

    let (min_elo, max_elo) = elo_range(sampled.points.iter().map(|point| point.elo))?;
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Sampled Winrate over Time"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(x_min..x_max, 0.0f64..1.0f64)?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(6)
        .y_label_formatter(&|rate| format!("{:.0}%", rate * 100.0))
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    for player_points in sampled.points.as_chunks::<WINRATE_POINTS_PER_PLAYER>().0 {
        chart.draw_series(LineSeries::new(
            player_points
                .iter()
                .map(|point| (point.elo, point.win_rate)),
            BLUE_600.mix(0.72).stroke_width(1),
        ))?;
    }
    chart.draw_series(sampled.points.iter().map(|point| {
        Circle::new(
            (point.elo, point.win_rate),
            MARKER_RADIUS,
            progress_color(point.progress).mix(0.15).filled(),
        )
    }))?;
    draw_progress_legend(&chart, x_min, x_max)?;
    root.present()?;
    Ok(())
}
