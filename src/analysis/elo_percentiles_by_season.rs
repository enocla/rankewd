use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels, percentile};
use crate::plot::{
    BASE_800, Line, LineChart, SERIES_COLORS, draw_line_chart, integer_tick, season_span,
    season_tick, value_range,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/elo_percentiles_by_season.png";
/// Rated matches a player needs before their final Elo counts.
const MIN_RATED_MATCHES: u32 = 10;
const PERCENTILES: [f64; 6] = [0.1, 0.25, 0.5, 0.75, 0.9, 0.99];

pub(super) struct EloPercentilesBySeason;

impl Analysis for EloPercentilesBySeason {
    fn name(&self) -> &'static str {
        "elo-percentiles-by-season"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut finals: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
        for ((season, _), rating) in duels.ratings() {
            if rating.rated_matches >= MIN_RATED_MATCHES {
                finals
                    .entry(season)
                    .or_default()
                    .push(f64::from(rating.final_elo));
            }
        }
        if finals.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no rated players").into());
        }
        for values in finals.values_mut() {
            values.sort_unstable_by(f64::total_cmp);
        }
        let seasons: Vec<u32> = finals.keys().copied().collect();

        let mut lines: Vec<Line> = PERCENTILES
            .iter()
            .enumerate()
            .rev()
            .map(|(index, fraction)| {
                Line::new(
                    format!("p{:.0}", fraction * 100.0),
                    SERIES_COLORS[index],
                    finals
                        .values()
                        .enumerate()
                        .map(|(position, values)| (position as f64, percentile(values, *fraction)))
                        .collect(),
                )
            })
            .collect();
        lines.insert(
            0,
            Line::new(
                "Highest",
                BASE_800,
                finals
                    .values()
                    .enumerate()
                    .map(|(position, values)| (position as f64, values[values.len() - 1]))
                    .collect(),
            ),
        );
        let y_range = value_range(
            lines
                .iter()
                .flat_map(|line| line.points.iter().map(|p| p.1)),
            0.05,
            0.35,
        );
        let medians: Vec<(u32, f64)> = finals
            .iter()
            .map(|(season, values)| (*season, percentile(values, 0.5)))
            .collect();
        let lowest = medians
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, 0.0));
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        let last = seasons.len() as f64 - 1.0;
        draw_line_chart(
            &output,
            &LineChart {
                title: format!(
                    "{}: Median Final Elo Bottomed at {:.0} in S{}",
                    season_span(&seasons),
                    lowest.1,
                    lowest.0
                ),
                x_desc: "Season",
                y_desc: "Final Elo",
                x_range: -0.3..last + 0.3,
                y_range,
                x_labels: seasons.len() * 2 + 1,
                x_format: &|position| season_tick(&seasons, *position),
                y_format: &integer_tick,
                lines,
                reference: None,
            },
        )?;

        let mut report = AnalysisReport::default();
        for (season, values) in &finals {
            report.line(format!(
                "Season {season}: {} players; p10 {:.0}, median {:.0}, p90 {:.0}, p99 {:.0}, top {:.0}",
                values.len(),
                percentile(values, 0.1),
                percentile(values, 0.5),
                percentile(values, 0.9),
                percentile(values, 0.99),
                values[values.len() - 1]
            ));
        }
        report.line(format!("  Elo percentiles by season: {}", output.display()));
        Ok(report)
    }
}
