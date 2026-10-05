use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BLUE_600, Line, LineChart, RED_600, draw_line_chart, integer_tick, season_span, value_range,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/deaths_versus_elo.png";
/// Width of one pre-match Elo bracket.
const BRACKET: i32 = 100;
const MIN_BRACKET_MATCHES: u64 = 500;

pub(super) struct DeathsVersusElo;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: u64,
    deaths: u64,
    deathless: u64,
}

impl Analysis for DeathsVersusElo {
    fn name(&self) -> &'static str {
        "deaths-versus-elo"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns {
                timelines: true,
                ..DuelColumns::default()
            },
        )?;
        // (bracket, won) -> tally
        let mut tallies: BTreeMap<(i32, bool), Tally> = BTreeMap::new();
        for duel in duels.duels.iter().filter(|duel| duel.has_timeline) {
            for (index, side) in duel.sides.iter().enumerate() {
                let (Some(won), Some(elo)) = (duel.won(index), side.elo_before) else {
                    continue;
                };
                let tally = tallies
                    .entry((elo.div_euclid(BRACKET) * BRACKET, won))
                    .or_default();
                tally.matches += 1;
                tally.deaths += u64::from(side.deaths);
                tally.deathless += u64::from(side.deaths == 0);
            }
        }
        let line = |won: bool| -> Vec<(f64, f64)> {
            tallies
                .iter()
                .filter(|((_, side_won), tally)| {
                    *side_won == won && tally.matches >= MIN_BRACKET_MATCHES
                })
                .map(|((bracket, _), tally)| {
                    (
                        f64::from(*bracket) + f64::from(BRACKET) / 2.0,
                        tally.deaths as f64 / tally.matches as f64,
                    )
                })
                .collect()
        };
        let (winners, losers) = (line(true), line(false));
        if winners.is_empty() || losers.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no timeline deaths").into());
        }
        let x_range = value_range(winners.iter().chain(&losers).map(|p| p.0), 0.03, 0.03);
        let y_range = value_range(winners.iter().chain(&losers).map(|p| p.1), 0.1, 0.35);

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!("{}: Deaths per Match by Elo", season_span(&seasons)),
                x_desc: "Pre-match Elo",
                y_desc: "Mean deaths per match",
                x_range,
                y_range: 0.0..y_range.end,
                x_labels: 10,
                x_format: &integer_tick,
                y_format: &|value| format!("{value:.1}"),
                lines: vec![
                    Line::new("Losers", RED_600, losers),
                    Line::new("Winners", BLUE_600, winners),
                ],
                reference: None,
            },
        )?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: timeline death events per player and match",
            season_span(&seasons)
        ));
        for ((bracket, won), tally) in &tallies {
            if tally.matches >= MIN_BRACKET_MATCHES {
                report.line(format!(
                    "  Elo {bracket:>4} {}: {:.2} deaths, {:.1}% deathless, {} matches",
                    if *won { "winners" } else { "losers " },
                    tally.deaths as f64 / tally.matches as f64,
                    100.0 * tally.deathless as f64 / tally.matches as f64,
                    tally.matches
                ));
            }
        }
        report.line(format!("  Deaths vs Elo: {}", output.display()));
        Ok(report)
    }
}
