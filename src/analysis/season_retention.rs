use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_800, CELL_FONT_SIZE, FONT_FAMILY, canvas, chart_builder, heat_color, season_span,
    season_tick, style_mesh, text_on,
};
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/season_retention.png";
/// Matches a player needs in a season to count as one of its players.
const MIN_MATCHES: usize = 10;

pub(super) struct SeasonRetention;

impl Analysis for SeasonRetention {
    fn name(&self) -> &'static str {
        "season-retention"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut players: BTreeMap<u32, HashSet<u128>> = BTreeMap::new();
        for ((season, uuid), appearances) in duels.appearances() {
            if appearances.len() >= MIN_MATCHES {
                players.entry(season).or_default().insert(uuid);
            }
        }
        if players.len() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "season retention needs at least two seasons",
            )
            .into());
        }
        let seasons: Vec<u32> = players.keys().copied().collect();
        let sets: Vec<&HashSet<u128>> = players.values().collect();
        // matrix[row][column]: share of the row season's players who also
        // played the column season.
        let matrix: Vec<Vec<f64>> = sets
            .iter()
            .map(|row| {
                sets.iter()
                    .map(|column| {
                        row.iter().filter(|uuid| column.contains(uuid)).count() as f64
                            / row.len().max(1) as f64
                    })
                    .collect()
            })
            .collect();

        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_chart(&output, &seasons, &matrix)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: players with at least {MIN_MATCHES} matches in a season",
            season_span(&seasons)
        ));
        for (index, season) in seasons.iter().enumerate() {
            let next = matrix[index].get(index + 1).copied();
            report.line(format!(
                "  S{season}: {} players{}",
                sets[index].len(),
                next.map_or_else(String::new, |share| format!(
                    ", {:.1}% played S{}",
                    share * 100.0,
                    seasons[index + 1]
                ))
            ));
        }
        report.line(format!("  Season retention: {}", output.display()));
        Ok(report)
    }
}

fn draw_chart(path: &Path, seasons: &[u32], matrix: &[Vec<f64>]) -> Result<(), Box<dyn Error>> {
    let size = seasons.len() as f64;
    let root = canvas(path)?;
    let mut chart = chart_builder(
        &root,
        &format!("{}: Who Comes Back Next Season?", season_span(seasons)),
    )
    .build_cartesian_2d(-0.5f64..size - 0.5, -0.5f64..size - 0.5)?;
    style_mesh(
        &mut chart.configure_mesh(),
        "Also played in season",
        "Players of season",
    )
    .disable_mesh()
    .x_labels(seasons.len() * 2 + 1)
    .y_labels(seasons.len() * 2 + 1)
    .x_label_formatter(&|position| season_tick(seasons, *position))
    .y_label_formatter(&|position| season_tick(seasons, *position))
    .draw()?;

    // The diagonal is 100% by definition; scale colors to the off-diagonal.
    let max_off = matrix
        .iter()
        .enumerate()
        .flat_map(|(row, values)| {
            values
                .iter()
                .enumerate()
                .filter(move |(column, _)| *column != row)
                .map(|(_, value)| *value)
        })
        .fold(0.0, f64::max)
        .max(f64::EPSILON);
    let centered = Pos::new(HPos::Center, VPos::Center);
    for (row, values) in matrix.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            let (x, y) = (column as f64, row as f64);
            let fill = if row == column {
                BASE_800
            } else {
                heat_color(value / max_off)
            };
            chart.draw_series(std::iter::once(Rectangle::new(
                [(x - 0.5, y - 0.5), (x + 0.5, y + 0.5)],
                fill.filled(),
            )))?;
            let label = if row == column {
                "—".to_owned()
            } else {
                format!("{:.0}%", value * 100.0)
            };
            chart.draw_series(std::iter::once(Text::new(
                label,
                (x, y),
                (FONT_FAMILY, CELL_FONT_SIZE)
                    .into_font()
                    .color(&text_on(fill))
                    .pos(centered),
            )))?;
        }
    }
    root.present()?;
    Ok(())
}
