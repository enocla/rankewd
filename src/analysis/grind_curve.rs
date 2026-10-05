use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    Line, LineChart, draw_line_chart, integer_tick, progress_color, season_span, value_range,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/grind_curve.png";
/// Rated matches every charted player reaches, so each curve follows one
/// fixed cohort from its first rated match to this one.
const COHORT_MATCHES: usize = 300;
const MIN_COHORT: usize = 20;

pub(super) struct GrindCurve;

impl Analysis for GrindCurve {
    fn name(&self) -> &'static str {
        "grind-curve"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        // Per season: summed Elo gain at each rated-match index, and cohort size.
        let mut by_season: BTreeMap<u32, (Vec<f64>, usize, f64)> = BTreeMap::new();
        for ((season, _), appearances) in duels.appearances() {
            let elos: Vec<i32> = appearances
                .iter()
                .filter_map(|appearance| duels.duels[appearance.duel].elo_after(appearance.side))
                .take(COHORT_MATCHES)
                .collect();
            if elos.len() < COHORT_MATCHES {
                continue;
            }
            let (sums, players, start) = by_season
                .entry(season)
                .or_insert_with(|| (vec![0.0; COHORT_MATCHES], 0, 0.0));
            *players += 1;
            *start += f64::from(elos[0]);
            for (index, elo) in elos.iter().enumerate() {
                sums[index] += f64::from(elo - elos[0]);
            }
        }
        by_season.retain(|_, (_, players, _)| *players >= MIN_COHORT);
        if by_season.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no season had enough players with 300 rated matches",
            )
            .into());
        }

        let count = by_season.len();
        let lines: Vec<Line> = by_season
            .iter()
            .enumerate()
            .map(|(position, (season, (sums, players, _)))| {
                let mut line = Line::new(
                    format!("S{season} ({players} players)"),
                    progress_color(position as f64 / (count.max(2) - 1) as f64),
                    sums.iter()
                        .enumerate()
                        .map(|(index, sum)| ((index + 1) as f64, sum / *players as f64))
                        .collect(),
                );
                line.markers = false;
                line
            })
            .collect();
        let y_range = value_range(
            lines
                .iter()
                .flat_map(|line| line.points.iter().map(|p| p.1)),
            0.05,
            0.9,
        );
        let seasons: Vec<u32> = by_season.keys().copied().collect();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!("{}: Elo Gained by Grinding", season_span(&seasons)),
                x_desc: "Rated matches played in season",
                y_desc: "Mean Elo vs first match",
                x_range: 1.0..COHORT_MATCHES as f64,
                y_range,
                x_labels: 7,
                x_format: &integer_tick,
                y_format: &|value| format!("{value:+.0}"),
                lines,
                reference: Some((0.0, "Starting Elo")),
            },
        )?;

        let mut report = AnalysisReport::default();
        for (season, (sums, players, start)) in &by_season {
            let at = |matches: usize| sums[matches - 1] / *players as f64;
            report.line(format!(
                "Season {season}: {players} players with {COHORT_MATCHES}+ rated matches, mean start {:.0}; gain {:+.0} by match 50, {:+.0} by 100, {:+.0} by {COHORT_MATCHES}",
                start / *players as f64,
                at(50),
                at(100),
                at(COHORT_MATCHES)
            ));
        }
        report.line(format!("  Grind curve: {}", output.display()));
        Ok(report)
    }
}
