use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct ForfeitCoverage;
impl Analysis for ForfeitCoverage {
    fn name(&self) -> &'static str {
        "forfeit-coverage"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/forfeit_coverage.png");
        let mut report = AnalysisReport::default();
        report.line("Match results by season");
        report.line("Descriptive coverage; missing-clock outcomes retained; malformed rows excluded, synthetic decay excluded. A selected season 12 snapshot is partial.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        for (part, label) in [
            (0, "No player completes"),
            (1, "A player forfeits"),
            (2, "Draw"),
            (3, "Win with no forfeit record"),
        ] {
            let points: Vec<_> = data
                .audits
                .iter()
                .map(|a| {
                    let x = f64::from(a.season);
                    let n = match part {
                        0 => a.flagged,
                        1 => a.explicit_quit,
                        2 => a.draws,
                        _ => a.unknown_quit,
                    };
                    let y = n as f64 / a.played.max(1) as f64;
                    report.line(format!(
                        "  S{} {label}: {n}/{} ({:.2}%)",
                        a.season,
                        a.played,
                        y * 100.0
                    ));
                    csv.push(format!("{label},{x},{y},{y},{y},{},0", a.played));
                    ResearchEstimate {
                        x,
                        mean: y,
                        low: y,
                        high: y,
                        n: a.played,
                        players: 0,
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
        let y_range = 0.0..1.0;
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Match results by season",
                x_desc: "Season",
                y_desc: "Match rate",
                x_range: f64::from(data.audits.first().map_or(1, |a| a.season)) - 0.5
                    ..f64::from(data.audits.last().map_or(1, |a| a.season)) + 0.5,
                y_range,
                x_format: &|x: &f64| {
                    if (x.round() - x).abs() < 0.01 {
                        format!("S{x:.0}")
                    } else {
                        String::new()
                    }
                },
                y_format: &percent_tick,
                x_labels: data.audits.len() + 2,
                curves,
                reference: None,
            },
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
