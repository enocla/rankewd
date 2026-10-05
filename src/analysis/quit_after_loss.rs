use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, SESSION_GAP_SECONDS, load_duels, session_slots};
use crate::plot::{
    Line, LineChart, RED_600, draw_line_chart, integer_tick, season_span, value_range,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/quit_after_loss.png";
/// Width of one pre-match Elo bracket.
const BRACKET: i32 = 100;
const MIN_BRACKET_MATCHES: u64 = 500;

pub(super) struct QuitAfterLoss;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: u64,
    session_ends: u64,
}

impl Tally {
    fn rate(self) -> f64 {
        self.session_ends as f64 / self.matches.max(1) as f64
    }
}

impl Analysis for QuitAfterLoss {
    fn name(&self) -> &'static str {
        "quit-after-loss"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        // (bracket, won) -> tally
        let mut tallies: BTreeMap<(i32, bool), Tally> = BTreeMap::new();
        let mut overall = [Tally::default(); 2];
        for appearances in duels.appearances().values() {
            for slot in session_slots(&duels.duels, appearances) {
                let duel = &duels.duels[slot.appearance.duel];
                let side = slot.appearance.side;
                let (Some(won), Some(elo)) = (duel.won(side), duel.sides[side].elo_before) else {
                    continue;
                };
                let bracket = elo.div_euclid(BRACKET) * BRACKET;
                for tally in [
                    tallies.entry((bracket, won)).or_default(),
                    &mut overall[usize::from(won)],
                ] {
                    tally.matches += 1;
                    tally.session_ends += u64::from(slot.last);
                }
            }
        }
        let brackets: Vec<i32> = tallies
            .keys()
            .map(|(bracket, _)| *bracket)
            .filter(|bracket| {
                [true, false].iter().all(|won| {
                    tallies
                        .get(&(*bracket, *won))
                        .is_some_and(|tally| tally.matches >= MIN_BRACKET_MATCHES)
                })
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if brackets.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no rated matches").into());
        }
        // How much likelier a session is to end after a loss than after a win.
        let ratio: Vec<(f64, f64)> = brackets
            .iter()
            .map(|bracket| {
                (
                    f64::from(*bracket) + f64::from(BRACKET) / 2.0,
                    tallies[&(*bracket, false)].rate()
                        / tallies[&(*bracket, true)].rate().max(f64::EPSILON),
                )
            })
            .collect();
        let x_range = value_range(ratio.iter().map(|p| p.0), 0.03, 0.03);
        let y_range = value_range(ratio.iter().map(|p| p.1).chain([1.0]), 0.2, 0.5);
        let (first, last) = (ratio[0], ratio[ratio.len() - 1]);

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!("{}: Do Losses Send Players Home?", season_span(&seasons)),
                x_desc: "Pre-match Elo",
                y_desc: "Log-off rate, loss ÷ win",
                x_range,
                y_range,
                x_labels: 10,
                x_format: &integer_tick,
                y_format: &|value| format!("{value:.2}x"),
                lines: vec![Line::new(
                    "Session-end rate after a loss / after a win",
                    RED_600,
                    ratio,
                )],
                reference: Some((1.0, "No difference")),
            },
        )?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: a session ends when no match starts within {} minutes of the last one ending",
            season_span(&seasons),
            SESSION_GAP_SECONDS / 60
        ));
        report.line(format!(
            "  A loss ends a session {:.2}x as often as a win at Elo {:.0}, {:.2}x at Elo {:.0}",
            first.1, first.0, last.1, last.0
        ));
        report.line(format!(
            "  Session ended after {:.1}% of losses and {:.1}% of wins",
            overall[0].rate() * 100.0,
            overall[1].rate() * 100.0
        ));
        for bracket in &brackets {
            report.line(format!(
                "  Elo {bracket:>4}: after loss {:.1}%, after win {:.1}%",
                tallies[&(*bracket, false)].rate() * 100.0,
                tallies[&(*bracket, true)].rate() * 100.0
            ));
        }
        report.line(format!("  Quit after loss: {}", output.display()));
        Ok(report)
    }
}
