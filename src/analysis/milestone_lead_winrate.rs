use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, MILESTONES, load_duels};
use crate::plot::{
    Line, LineChart, SERIES_COLORS, draw_line_chart, integer_tick, percent_tick, season_span,
};
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/milestone_lead_winrate.png";
/// Width of one lead bin, in seconds.
const LEAD_BIN_SECONDS: u32 = 15;
/// Largest lead charted, in seconds.
const MAX_LEAD_SECONDS: u32 = 240;
const MIN_BIN_MATCHES: u64 = 300;

pub(super) struct MilestoneLeadWinrate;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: u64,
    leader_wins: u64,
}

impl Analysis for MilestoneLeadWinrate {
    fn name(&self) -> &'static str {
        "milestone-lead-winrate"
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
        let bins = usize::try_from(MAX_LEAD_SECONDS / LEAD_BIN_SECONDS)?;
        let mut tallies = vec![vec![Tally::default(); bins]; MILESTONES.len()];
        let mut overall = vec![Tally::default(); MILESTONES.len()];
        for duel in &duels.duels {
            let Some(winner) = duel.winner else { continue };
            for (index, tally_row) in tallies.iter_mut().enumerate() {
                let (Some(first), Some(second)) =
                    (duel.sides[0].splits[index], duel.sides[1].splits[index])
                else {
                    continue;
                };
                if first == second {
                    continue;
                }
                let leader = usize::from(second < first);
                let lead_ms = first.get().abs_diff(second.get());
                let won = u64::from(winner == leader);
                overall[index].matches += 1;
                overall[index].leader_wins += won;
                let bin = usize::try_from(lead_ms / 1000 / LEAD_BIN_SECONDS)?;
                if let Some(tally) = tally_row.get_mut(bin) {
                    tally.matches += 1;
                    tally.leader_wins += won;
                }
            }
        }
        if overall.iter().all(|tally| tally.matches == 0) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no timeline splits").into());
        }

        let lines: Vec<Line> = MILESTONES
            .iter()
            .enumerate()
            .map(|(index, milestone)| {
                Line::new(
                    format!(
                        "{} ({:.0}% overall)",
                        milestone.label(),
                        100.0 * overall[index].leader_wins as f64
                            / overall[index].matches.max(1) as f64
                    ),
                    SERIES_COLORS[index],
                    tallies[index]
                        .iter()
                        .enumerate()
                        .filter(|(_, tally)| tally.matches >= MIN_BIN_MATCHES)
                        .map(|(bin, tally)| {
                            (
                                (bin as f64 + 0.5) * f64::from(LEAD_BIN_SECONDS),
                                tally.leader_wins as f64 / tally.matches as f64,
                            )
                        })
                        .collect(),
                )
            })
            .collect();

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!("{}: Which Split Lead Wins the Race?", season_span(&seasons)),
                x_desc: "Lead at the milestone (seconds)",
                y_desc: "Leader's win rate",
                x_range: 0.0..f64::from(MAX_LEAD_SECONDS),
                y_range: 0.45..1.12,
                x_labels: 9,
                x_format: &integer_tick,
                y_format: &|rate| {
                    if *rate > 1.0 + 1e-9 {
                        String::new()
                    } else {
                        percent_tick(rate)
                    }
                },
                lines,
                reference: Some((0.5, "Coin flip")),
            },
        )?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: win rate of the player who reached each milestone first",
            season_span(&seasons)
        ));
        for (index, milestone) in MILESTONES.iter().enumerate() {
            let lead_at = |seconds: u32| {
                tallies[index]
                    .get(usize::try_from(seconds / LEAD_BIN_SECONDS).unwrap_or(usize::MAX))
                    .filter(|tally| tally.matches >= MIN_BIN_MATCHES)
                    .map_or_else(
                        || "-".to_owned(),
                        |tally| {
                            format!(
                                "{:.1}%",
                                100.0 * tally.leader_wins as f64 / tally.matches as f64
                            )
                        },
                    )
            };
            report.line(format!(
                "  {:<12} leader won {:.1}% of {} matches; with a 0-15 s lead {}, 60-75 s {}, 120-135 s {}",
                milestone.label(),
                100.0 * overall[index].leader_wins as f64 / overall[index].matches.max(1) as f64,
                overall[index].matches,
                lead_at(0),
                lead_at(60),
                lead_at(120)
            ));
        }
        report.line(format!("  Milestone lead win rate: {}", output.display()));
        Ok(report)
    }
}
