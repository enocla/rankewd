use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{
    MIN_GAMES_FOR_WINRATE_PLOTS, PlayerSeasons, TemporalPoint, eligible_winrate_uuids, elo_range,
};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, PAPER, POINT_RADIUS,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_progress_legend, padded_bounds,
    progress_color,
};
use plotters::prelude::*;
use std::error::Error;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "winrate_over_time_season_";
const X_AXIS_TITLE: &str = "Elo at match time";
const Y_AXIS_TITLE: &str = "Cumulative win rate at match time";

pub(super) struct WinrateOverTime;

impl Analysis for WinrateOverTime {
    fn name(&self) -> &'static str {
        "winrate-over-time"
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
            let points = season.history.points(&uuids)?;
            if points.is_empty() {
                report.line(format!(
                    "Season {}: no rated decided matches to plot",
                    season.season
                ));
                continue;
            }
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &points)?;
            report.line(format!(
                "Season {}: {} players, {} rated decided matches",
                season.season,
                uuids.len(),
                points.len()
            ));
            report.line(format!("  Win rate over time: {}", path.display()));
        }
        Ok(report)
    }
}

fn draw_chart(path: &Path, season: u32, points: &[TemporalPoint]) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let (min_elo, max_elo) = elo_range(points.iter().map(|point| point.elo))?;
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Winrate vs Elo Over Time"),
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

    chart.draw_series(points.iter().map(|point| {
        Circle::new(
            (point.elo, point.win_rate),
            POINT_RADIUS,
            progress_color(point.progress).mix(0.50).filled(),
        )
    }))?;
    draw_progress_legend(&chart, x_min, x_max)?;
    root.present()?;
    Ok(())
}
