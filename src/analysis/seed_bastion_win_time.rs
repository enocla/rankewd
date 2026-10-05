use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{BASTION_TYPES, DuelColumns, SEED_TYPES, load_duels, median};
use crate::plot::{
    CELL_FONT_SIZE, FONT_FAMILY, canvas, chart_builder, format_clock, heat_color, season_span,
    style_mesh, text_on,
};
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/seed_bastion_win_time.png";
const MIN_CELL_WINS: usize = 200;

pub(super) struct SeedBastionWinTime;

/// Median winning time and sample size of one seed type and bastion pairing.
type Cell = Option<(f64, usize)>;

impl Analysis for SeedBastionWinTime {
    fn name(&self) -> &'static str {
        "seed-bastion-win-time"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut times = vec![vec![Vec::new(); BASTION_TYPES.len()]; SEED_TYPES.len()];
        for duel in &duels.duels {
            if let (Some(_), Some(time), Some(seed), Some(bastion)) = (
                duel.winner,
                duel.duration_ms,
                duel.seed_type,
                duel.bastion_type,
            ) && time > 0
            {
                times[seed][bastion].push(f64::from(time));
            }
        }
        let cells: Vec<Vec<Cell>> = times
            .iter_mut()
            .map(|row| {
                row.iter_mut()
                    .map(|values| {
                        let wins = values.len();
                        (wins >= MIN_CELL_WINS)
                            .then(|| median(values).map(|median| (median, wins)))
                            .flatten()
                    })
                    .collect()
            })
            .collect();
        let known: Vec<f64> = cells
            .iter()
            .flatten()
            .flatten()
            .map(|cell| cell.0)
            .collect();
        if known.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no seed and bastion type had enough wins",
            )
            .into());
        }
        let fastest = known.iter().copied().fold(f64::INFINITY, f64::min);
        let slowest = known.iter().copied().fold(0.0, f64::max);

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        let named = |target: f64| {
            cells
                .iter()
                .enumerate()
                .find_map(|(row, values)| {
                    values
                        .iter()
                        .any(|cell| cell.is_some_and(|(time, _)| time == target))
                        .then(|| pretty(SEED_TYPES[row]))
                })
                .unwrap_or_default()
        };
        let title = format!(
            "{}: {} {} vs {} {}",
            season_span(&seasons),
            named(fastest),
            format_clock(fastest),
            named(slowest),
            format_clock(slowest)
        );
        draw_chart(&output, &title, &cells, fastest, slowest)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: median winning time by overworld seed type and bastion",
            season_span(&seasons)
        ));
        for (seed, row) in SEED_TYPES.iter().zip(&cells) {
            let values: Vec<String> = BASTION_TYPES
                .iter()
                .zip(row)
                .map(|(bastion, cell)| match cell {
                    Some((time, wins)) => format!("{bastion} {} ({wins})", format_clock(*time)),
                    None => format!("{bastion} -"),
                })
                .collect();
            report.line(format!("  {seed:<15} {}", values.join(", ")));
        }
        report.line(format!(
            "  Fastest pairing {}, slowest {}",
            format_clock(fastest),
            format_clock(slowest)
        ));
        report.line(format!("  Seed and bastion win time: {}", output.display()));
        Ok(report)
    }
}

fn pretty(label: &str) -> String {
    label
        .split('_')
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_string() + &characters.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn draw_chart(
    path: &Path,
    title: &str,
    cells: &[Vec<Cell>],
    fastest: f64,
    slowest: f64,
) -> Result<(), Box<dyn Error>> {
    let root = canvas(path)?;
    let columns = BASTION_TYPES.len() as f64;
    let rows = SEED_TYPES.len() as f64;
    let mut chart = chart_builder(&root, title)
        .y_label_area_size(360)
        .build_cartesian_2d(-0.5f64..columns - 0.5, rows - 0.5..-0.5f64)?;
    let column_labels: Vec<String> = BASTION_TYPES.iter().map(|label| pretty(label)).collect();
    let row_labels: Vec<String> = SEED_TYPES.iter().map(|label| pretty(label)).collect();
    let category = |labels: &[String], value: f64| {
        let index = value.round();
        if (value - index).abs() < 1e-6 && index >= 0.0 {
            labels.get(index as usize).cloned().unwrap_or_default()
        } else {
            String::new()
        }
    };
    style_mesh(&mut chart.configure_mesh(), "Bastion", "Overworld seed")
        .disable_mesh()
        .x_labels(BASTION_TYPES.len() * 2 + 1)
        .y_labels(SEED_TYPES.len() * 2 + 1)
        .x_label_formatter(&|value| category(&column_labels, *value))
        .y_label_formatter(&|value| category(&row_labels, *value))
        .draw()?;

    let centered = Pos::new(HPos::Center, VPos::Center);
    let span = (slowest - fastest).max(1.0);
    for (row, values) in cells.iter().enumerate() {
        for (column, cell) in values.iter().enumerate() {
            let (x, y) = (column as f64, row as f64);
            let Some((time, wins)) = cell else { continue };
            // Darker cells are slower.
            let fill = heat_color(0.1 + 0.9 * (time - fastest) / span);
            chart.draw_series(std::iter::once(Rectangle::new(
                [(x - 0.5, y - 0.5), (x + 0.5, y + 0.5)],
                fill.filled(),
            )))?;
            let style = (FONT_FAMILY, CELL_FONT_SIZE + 12)
                .into_font()
                .color(&text_on(fill))
                .pos(centered);
            chart.draw_series(std::iter::once(Text::new(
                format_clock(*time),
                (x, y - 0.1),
                style.clone(),
            )))?;
            chart.draw_series(std::iter::once(Text::new(
                format!("{wins} wins"),
                (x, y + 0.22),
                (FONT_FAMILY, CELL_FONT_SIZE - 2)
                    .into_font()
                    .color(&text_on(fill))
                    .pos(centered),
            )))?;
        }
    }
    root.present()?;
    Ok(())
}
