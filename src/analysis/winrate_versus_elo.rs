use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{MIN_GAMES_FOR_WINRATE_PLOTS, PlayerSeason, PlayerSeasons, elo_range};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, GREEN_600, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER,
    POINT_RADIUS, TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend,
    padded_bounds,
};
use plotters::prelude::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "winrate_vs_elo_season_";
const WINRATE_BIN_SIZE: i64 = 100;
const MIN_WINRATE_BIN_PLAYERS: u64 = 20;
const X_AXIS_TITLE: &str = "Latest Elo in season";
const Y_AXIS_TITLE: &str = "Win rate (decided matches)";

pub(super) struct WinrateVersusElo;

impl Analysis for WinrateVersusElo {
    fn name(&self) -> &'static str {
        "winrate-versus-elo"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load_summaries(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;
        let seasons = data.seasons()?;

        let mut report = AnalysisReport::default();
        for season in seasons {
            let players: Vec<_> = season
                .players
                .iter()
                .copied()
                .filter(|player| {
                    player.games_played >= MIN_GAMES_FOR_WINRATE_PLOTS
                        && player.win_rate().is_some()
                })
                .collect();
            if players.is_empty() {
                report.line(format!(
                    "Season {}: no players with at least {MIN_GAMES_FOR_WINRATE_PLOTS} matches",
                    season.season
                ));
                continue;
            }
            let points = scatter_points(&players)?;
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &points)?;
            report.line(format!(
                "Season {}: {} players with at least {MIN_GAMES_FOR_WINRATE_PLOTS} matches",
                season.season,
                players.len()
            ));
            report.line(format!("  Win rate vs Elo: {}", path.display()));
        }
        Ok(report)
    }
}

fn scatter_points(players: &[&PlayerSeason]) -> Result<Vec<(i64, f64)>, Box<dyn Error>> {
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
    Ok(points)
}

/// Mean win rate per Elo bin, keeping only bins with enough players.
fn mean_by_elo(points: &[(i64, f64)]) -> Vec<(i64, f64)> {
    let mut bins = BTreeMap::<i64, (f64, u64)>::new();
    for (elo, win_rate) in points {
        let (sum, count) = bins
            .entry(elo.div_euclid(WINRATE_BIN_SIZE) * WINRATE_BIN_SIZE)
            .or_default();
        *sum += win_rate;
        *count += 1;
    }
    bins.into_iter()
        .filter_map(|(bin, (sum, count))| {
            (count >= MIN_WINRATE_BIN_PLAYERS)
                .then_some((bin + WINRATE_BIN_SIZE / 2, sum / count as f64))
        })
        .collect()
}

fn draw_chart(path: &Path, season: u32, points: &[(i64, f64)]) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let (min_elo, max_elo) = elo_range(points.iter().map(|(elo, _)| *elo))?;
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);
    let means = mean_by_elo(points);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Player Winrate vs Elo"),
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
            means.iter().copied(),
            GREEN_600.stroke_width(3),
        ))?
        .label(format!(
            "Mean by Elo ({MIN_WINRATE_BIN_PLAYERS}+ players per bin)"
        ))
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + LEGEND_WIDTH, y)],
                GREEN_600.stroke_width(3),
            )
        });
    chart.draw_series(
        means
            .iter()
            .map(|(elo, win_rate)| Circle::new((*elo, *win_rate), 4, GREEN_600.filled())),
    )?;
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
