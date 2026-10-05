use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_800, BLUE_600, Line, LineChart, draw_line_chart, integer_tick, percent_tick, season_span,
    value_range,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/upsets_by_elo.png";
/// Pre-match gaps of charted matches: large enough for a clear favourite.
const MIN_GAP: i32 = 100;
const MAX_GAP: i32 = 200;
/// Width of one bracket of the pair's mean Elo.
const BRACKET: i32 = 100;
const MIN_BRACKET_MATCHES: u64 = 1000;
/// Lowest mean Elo charted; below it most ratings are still unsettled.
const MIN_PAIR_ELO: i32 = 800;

pub(super) struct UpsetsByElo;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: u64,
    upsets: u64,
    expected: f64,
}

impl Analysis for UpsetsByElo {
    fn name(&self) -> &'static str {
        "upsets-by-elo"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut brackets: BTreeMap<i32, Tally> = BTreeMap::new();
        for duel in &duels.duels {
            let (Some((first, second)), Some(winner)) = (duel.elos(), duel.winner) else {
                continue;
            };
            let gap = (first - second).abs();
            if !(MIN_GAP..=MAX_GAP).contains(&gap) {
                continue;
            }
            let underdog = usize::from(second < first);
            let bracket = (first + second) / 2 / BRACKET * BRACKET;
            let tally = brackets.entry(bracket).or_default();
            tally.matches += 1;
            tally.upsets += u64::from(winner == underdog);
            tally.expected += duel.expected_score(underdog).unwrap_or(0.5);
        }
        brackets.retain(|bracket, tally| {
            *bracket >= MIN_PAIR_ELO && tally.matches >= MIN_BRACKET_MATCHES
        });
        if brackets.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no rated matches").into());
        }

        let center = |bracket: i32| f64::from(bracket) + f64::from(BRACKET) / 2.0;
        let actual: Vec<(f64, f64)> = brackets
            .iter()
            .map(|(bracket, tally)| (center(*bracket), tally.upsets as f64 / tally.matches as f64))
            .collect();
        let expected: Vec<(f64, f64)> = brackets
            .iter()
            .map(|(bracket, tally)| (center(*bracket), tally.expected / tally.matches as f64))
            .collect();
        let x_range = value_range(actual.iter().map(|point| point.0), 0.04, 0.04);
        let y_range = value_range(
            actual.iter().chain(&expected).map(|point| point.1),
            0.1,
            0.45,
        );
        let lowest_rate = actual.iter().map(|point| point.1).fold(1.0, f64::min);
        let highest_rate = actual.last().map_or(0.0, |point| point.1);
        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!(
                    "{}: Upsets Climb from {:.0}% to {:.0}% at the Top",
                    season_span(&seasons),
                    lowest_rate * 100.0,
                    highest_rate * 100.0
                ),
                x_desc: "Mean Elo of the pair",
                y_desc: "Underdog win rate",
                x_range,
                y_range,
                x_labels: 10,
                x_format: &integer_tick,
                y_format: &percent_tick,
                lines: vec![
                    Line::new("Elo formula expectation", BASE_800, expected),
                    Line::new(
                        format!("Observed ({MIN_GAP}–{MAX_GAP} Elo underdog)"),
                        BLUE_600,
                        actual,
                    ),
                ],
                reference: None,
            },
        )?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: matches with a {MIN_GAP}-{MAX_GAP} pre-match Elo gap, by mean Elo",
            season_span(&seasons)
        ));
        for (bracket, tally) in &brackets {
            report.line(format!(
                "  {bracket:>4}-{:<4}: underdog won {:.1}% (expected {:.1}%) of {} matches",
                bracket + BRACKET,
                100.0 * tally.upsets as f64 / tally.matches as f64,
                100.0 * tally.expected / tally.matches as f64,
                tally.matches
            ));
        }
        report.line(format!("  Upsets by Elo: {}", output.display()));
        Ok(report)
    }
}
