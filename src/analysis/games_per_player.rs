use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::PlayerSeasons;
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend,
};
use plotters::prelude::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "games_per_player_season_";
const X_AXIS_TITLE: &str = "Games played in season (log scale)";
const Y_AXIS_TITLE: &str = "Players";

pub(super) struct GamesPerPlayer;

#[derive(Clone, Copy)]
struct DistributionSummary {
    mean: f64,
    median: f64,
}

impl Analysis for GamesPerPlayer {
    fn name(&self) -> &'static str {
        "games-per-player"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;

        let mut report = AnalysisReport::default();
        for season in data.all_player_seasons() {
            let games: Vec<_> = season
                .players
                .iter()
                .map(|player| player.games_played)
                .collect();
            let summary = summarize(&games)?;
            let bins = count_bins(&games);
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &bins)?;
            report.line(format!(
                "Season {}: {} players; mean games played: {:.2}; median games played: {:.2}",
                season.season,
                season.players.len(),
                summary.mean,
                summary.median
            ));
            report.line(format!("  Games per player histogram: {}", path.display()));
        }
        Ok(report)
    }
}

fn summarize(games: &[u64]) -> Result<DistributionSummary, Box<dyn Error>> {
    if games.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no players to summarize").into());
    }

    let total: u128 = games.iter().copied().map(u128::from).sum();
    let mut sorted = games.to_vec();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    let median = if sorted.len().is_multiple_of(2) {
        (u128::from(sorted[middle - 1]) + u128::from(sorted[middle])) as f64 / 2.0
    } else {
        sorted[middle] as f64
    };

    Ok(DistributionSummary {
        mean: total as f64 / sorted.len() as f64,
        median,
    })
}

/// Groups game counts into [1, 2), [2, 4), ... bins.
fn count_bins(games: &[u64]) -> BTreeMap<u64, u64> {
    let mut bins = BTreeMap::new();
    for games in games {
        let bin_start = games
            .checked_next_power_of_two()
            .expect("a games-played count always has a next power of two");
        *bins.entry(bin_start).or_default() += 1;
    }
    bins
}

fn draw_chart(path: &Path, season: u32, bins: &BTreeMap<u64, u64>) -> Result<(), Box<dyn Error>> {
    if bins.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no games played to plot").into());
    }
    create_parent(path)?;

    let last_bin = *bins.last_key_value().expect("non-empty bins").0;
    let x_end = last_bin.saturating_mul(2);
    let mut x_key_points = vec![1];
    let mut key_point = 10;
    while key_point < x_end {
        x_key_points.push(key_point);
        key_point = key_point.saturating_mul(10);
    }
    let max_count = *bins.values().max().expect("non-empty bins");
    let y_end = max_count.saturating_add(max_count / 10).saturating_add(1);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Games Played per Player"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(
            (1u64..x_end).log_scale().with_key_points(x_key_points),
            0u64..y_end,
        )?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
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
                [(*bin, 0), (bin.saturating_mul(2).min(x_end), *count)],
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
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
