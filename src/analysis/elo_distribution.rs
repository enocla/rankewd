use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{PlayerSeason, PlayerSeasons};
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

const OUTPUT_FILE_PREFIX: &str = "elo_distribution_season_";
const ELO_BIN_SIZE: i64 = 5;
const X_AXIS_TITLE: &str = "Latest Elo in season (5-point bins)";
const Y_AXIS_TITLE: &str = "Players";

pub(super) struct EloDistribution;

impl Analysis for EloDistribution {
    fn name(&self) -> &'static str {
        "elo-distribution"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;
        let seasons = data.seasons()?;

        let mut report = AnalysisReport::default();
        for season in seasons {
            let bins = count_bins(&season.players);
            if bins.is_empty() {
                return Err(
                    io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot").into(),
                );
            }
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            let max_count = *bins.values().max().expect("non-empty bins");
            draw_chart(&path, season.season, &bins, max_count)?;
            report.line(format!(
                "Season {}: {} rated players in {ELO_BIN_SIZE}-point Elo bins",
                season.season,
                season.players.len()
            ));
            report.line(format!("  Elo distribution: {}", path.display()));
        }
        Ok(report)
    }
}

fn count_bins(players: &[&PlayerSeason]) -> BTreeMap<i64, u64> {
    let mut bins = BTreeMap::<i64, u64>::new();
    for elo in players.iter().filter_map(|player| player.latest_elo) {
        *bins
            .entry(elo.div_euclid(ELO_BIN_SIZE) * ELO_BIN_SIZE)
            .or_default() += 1;
    }
    bins
}

fn draw_chart(
    path: &Path,
    season: u32,
    bins: &BTreeMap<i64, u64>,
    max_count: u64,
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let first_bin = *bins.first_key_value().expect("non-empty bins").0;
    let last_bin = *bins.last_key_value().expect("non-empty bins").0;
    let x_end = last_bin.saturating_add(ELO_BIN_SIZE);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Elo Distribution"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(first_bin..x_end, 0u64..(max_count + max_count / 10 + 1))?;

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
                [(*bin, 0), (bin.saturating_add(ELO_BIN_SIZE - 4), *count)],
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
