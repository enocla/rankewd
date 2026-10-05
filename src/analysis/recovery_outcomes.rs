use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct RecoveryOutcomes;
impl Analysis for RecoveryOutcomes {
    fn name(&self) -> &'static str {
        "recovery-outcomes"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/recovery_outcomes.png");
        let mut report = AnalysisReport::default();
        report.line("How Players Win rate after death");
        report.line("Use the first ordinary death in matches without your forfeit. A later split is not necessary. Ignore world resets. Show the 95% confidence interval for total wins.");
        report.line("A win without a completion does not require a recorded opponent forfeit.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        for (part, label) in [
            (0, "Win with a completion"),
            (1, "Win without completion"),
            (2, "Total wins"),
        ] {
            let points: Vec<_> = data
                .recovery
                .iter()
                .filter(|c| {
                    c.key.season == 0
                        && c.key.rank < 6
                        && c.key.kind == 0
                        && c.key.view == 0
                        && c.key.behavior == 1
                        && c.n >= 100
                })
                .map(|c| {
                    let x = f64::from(c.key.rank);
                    let y = match part {
                        0 => c.completion_wins as f64 / c.n as f64,
                        1 => (c.wins - c.completion_wins) as f64 / c.n as f64,
                        _ => c.rate(),
                    };
                    let (low, high) = if part == 2 { (c.low, c.high) } else { (y, y) };
                    csv.push(format!(
                        "{label},{x},{y},{low},{high},{},{}",
                        c.n, c.players
                    ));
                    report.line(format!(
                        "  {} {label}: {:.2}% ({}/{})",
                        RESEARCH_RANKS[c.key.rank as usize],
                        y * 100.0,
                        (y * c.n as f64).round(),
                        c.n
                    ));
                    ResearchEstimate {
                        x,
                        mean: y,
                        low,
                        high,
                        n: c.n,
                        players: c.players,
                    }
                })
                .collect();
            curves.push(ResearchCurve {
                label: label.into(),
                color: SERIES_COLORS[curves.len()],
                points,
            });
        }
        research_csv(
            &output.with_extension("csv"),
            "series,x,mean,low_95,high_95,observations,players",
            csv,
        )?;
        let y_range = 0.0..0.8;
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "How Players Win rate after death",
                x_desc: "Rank before the match",
                y_desc: "Win rate",
                x_range: -0.4..5.5,
                y_range,
                x_format: &|x: &f64| {
                    let i = x.round() as usize;
                    if (*x - i as f64).abs() < 0.01 && i < 6 {
                        RESEARCH_RANKS[i].to_owned()
                    } else {
                        String::new()
                    }
                },
                y_format: &percent_tick,
                x_labels: 10,
                curves,
                reference: Some((0.05, "5% limit")),
            },
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
