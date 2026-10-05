use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels, session_slots};
use crate::plot::{
    BLUE_600, Line, LineChart, draw_line_chart, integer_tick, season_span, value_range,
};
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/session_fatigue.png";
/// Session positions binned on their own; later matches join the last bin.
const MAX_POSITION: usize = 20;
const MIN_BIN_MATCHES: f64 = 500.0;

pub(super) struct SessionFatigue;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: f64,
    wins: f64,
    expected: f64,
    elo_change: f64,
}

impl Analysis for SessionFatigue {
    fn name(&self) -> &'static str {
        "session-fatigue"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut tallies = vec![Tally::default(); MAX_POSITION];
        for appearances in duels.appearances().values() {
            for slot in session_slots(&duels.duels, appearances) {
                let duel = &duels.duels[slot.appearance.duel];
                let side = slot.appearance.side;
                let (Some(won), Some(expected)) = (duel.won(side), duel.expected_score(side))
                else {
                    continue;
                };
                let tally = &mut tallies[slot.position.min(MAX_POSITION - 1)];
                tally.matches += 1.0;
                tally.wins += f64::from(u8::from(won));
                tally.expected += expected;
                tally.elo_change += f64::from(duel.sides[side].elo_change.unwrap_or(0));
            }
        }
        let points: Vec<(f64, f64)> = tallies
            .iter()
            .enumerate()
            .filter(|(_, tally)| tally.matches >= MIN_BIN_MATCHES)
            .map(|(index, tally)| {
                (
                    (index + 1) as f64,
                    (tally.wins - tally.expected) / tally.matches,
                )
            })
            .collect();
        if points.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no rated matches").into());
        }

        let seasons = duels.seasons();
        let y_range = value_range(points.iter().map(|p| p.1).chain([0.0]), 0.15, 0.5);
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!(
                    "{}: Session Openers Run {:+.1} Points vs Elo",
                    season_span(&seasons),
                    points[0].1 * 100.0
                ),
                x_desc: "Match number within session (30 min idle ends one)",
                y_desc: "Win rate vs Elo (points)",
                x_range: 0.5..MAX_POSITION as f64 + 0.5,
                y_range,
                x_labels: MAX_POSITION,
                x_format: &|value| {
                    let position = value.round() as usize;
                    if position == MAX_POSITION {
                        format!("{MAX_POSITION}+")
                    } else if position % 2 == 1 {
                        integer_tick(value)
                    } else {
                        String::new()
                    }
                },
                y_format: &|value| format!("{:+.1}", value * 100.0),
                lines: vec![Line::new(
                    "Win rate minus Elo expectation",
                    BLUE_600,
                    points.clone(),
                )],
                reference: Some((0.0, "As Elo predicts")),
            },
        )?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: sessions end after {} idle minutes",
            season_span(&seasons),
            crate::dataset::SESSION_GAP_SECONDS / 60
        ));
        for (index, tally) in tallies.iter().enumerate() {
            if tally.matches >= MIN_BIN_MATCHES {
                report.line(format!(
                    "  Match {:>2}{}: {:+.2} points vs expectation, mean Elo change {:+.2}, {:.0} matches",
                    index + 1,
                    if index + 1 == MAX_POSITION { "+" } else { " " },
                    100.0 * (tally.wins - tally.expected) / tally.matches,
                    tally.elo_change / tally.matches,
                    tally.matches
                ));
            }
        }
        report.line(format!("  Session fatigue: {}", output.display()));
        Ok(report)
    }
}
