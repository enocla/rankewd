use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_800, BLUE_600, RED_600, canvas, chart_builder, draw_series_legend, legend_box,
    season_span, style_mesh,
};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/streak_winrate.png";
/// Longest streak binned on its own; longer streaks join this bin.
const MAX_STREAK: i32 = 8;
const MIN_BIN_MATCHES: f64 = 200.0;

pub(super) struct StreakWinrate;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: f64,
    wins: f64,
    expected: f64,
}

impl Analysis for StreakWinrate {
    fn name(&self) -> &'static str {
        "streak-winrate"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        // Index `streak + MAX_STREAK`: negative streaks are losses in a row.
        let mut tallies = vec![Tally::default(); usize::try_from(2 * MAX_STREAK + 1)?];
        for appearances in duels.appearances().values() {
            let mut streak = 0i32;
            for appearance in appearances {
                let duel = &duels.duels[appearance.duel];
                let Some(won) = duel.won(appearance.side) else {
                    continue;
                };
                // A season opener has no streak yet, and its rating is still a
                // carried-over placement, so it is left out.
                if streak != 0
                    && let Some(expected) = duel.expected_score(appearance.side)
                {
                    let bin = &mut tallies[usize::try_from(streak + MAX_STREAK)?];
                    bin.matches += 1.0;
                    bin.wins += f64::from(u8::from(won));
                    bin.expected += expected;
                }
                streak = match (won, streak.signum()) {
                    (true, 1) => (streak + 1).min(MAX_STREAK),
                    (true, _) => 1,
                    (false, -1) => (streak - 1).max(-MAX_STREAK),
                    (false, _) => -1,
                };
            }
        }
        let bars: Vec<(i32, f64, f64)> = tallies
            .iter()
            .enumerate()
            .filter(|(_, tally)| tally.matches >= MIN_BIN_MATCHES)
            .map(|(index, tally)| {
                (
                    index as i32 - MAX_STREAK,
                    (tally.wins - tally.expected) / tally.matches,
                    tally.matches,
                )
            })
            .collect();
        if bars.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no rated matches").into());
        }

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_chart(&output, &season_span(&seasons), &bars)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: win rate above Elo expectation by the streak a player brings into a match",
            season_span(&seasons)
        ));
        for (streak, excess, matches) in &bars {
            report.line(format!(
                "  {:>+3} {}: {:+.2} points over {matches:.0} matches",
                streak,
                if *streak < 0 { "losses" } else { "wins" },
                excess * 100.0
            ));
        }
        report.line(format!("  Streak win rate: {}", output.display()));
        Ok(report)
    }
}

fn draw_chart(path: &Path, span: &str, bars: &[(i32, f64, f64)]) -> Result<(), Box<dyn Error>> {
    let limit = bars
        .iter()
        .map(|bar| bar.1.abs())
        .fold(0.0, f64::max)
        .max(0.005)
        * 1.35;
    let root = canvas(path)?;
    let edge = f64::from(MAX_STREAK) + 0.6;
    let mut chart = chart_builder(&root, &format!("{span}: Streaks Predict Wins Beyond Elo"))
        .build_cartesian_2d(-edge..edge, -limit..limit)?;
    style_mesh(
        &mut chart.configure_mesh(),
        "Streak going in (losses ← → wins)",
        "Win rate vs Elo (points)",
    )
    .disable_x_mesh()
    .x_labels(usize::try_from(2 * MAX_STREAK + 1)?)
    .x_label_formatter(&|value| {
        let streak = value.round() as i32;
        if (value - value.round()).abs() > 1e-6 {
            String::new()
        } else if streak.abs() == MAX_STREAK {
            format!("{}{MAX_STREAK}+", if streak < 0 { "L" } else { "W" })
        } else if streak == 0 {
            String::new()
        } else {
            format!("{}{}", if streak < 0 { "L" } else { "W" }, streak.abs())
        }
    })
    .y_label_formatter(&|value| {
        let points = (value * 100.0 * 10.0).round() / 10.0;
        if points == 0.0 {
            "0".to_owned()
        } else {
            format!("{points:+.0}")
        }
    })
    .draw()?;

    for (color, label, better) in [
        (BLUE_600, "Wins more than Elo predicts", true),
        (RED_600, "Wins less than Elo predicts", false),
    ] {
        chart
            .draw_series(bars.iter().filter(|bar| (bar.1 >= 0.0) == better).map(
                |(streak, excess, _)| {
                    let x = f64::from(*streak);
                    Rectangle::new([(x - 0.35, 0.0), (x + 0.35, *excess)], color.filled())
                },
            ))?
            .label(label)
            .legend(legend_box(color));
    }
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(-edge, 0.0), (edge, 0.0)],
        BASE_800.stroke_width(2),
    )))?;
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
