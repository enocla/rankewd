use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct DeathResetExclusions;
impl Analysis for DeathResetExclusions {
    fn name(&self) -> &'static str {
        "death-reset-exclusions"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/death_reset_exclusions.png");
        let mut report = AnalysisReport::default();
        report.line("Intentional deaths excluded");
        report.line("This figure counts excluded intentional spawnpoint deaths. Exclude these deaths from all death rates and continued play comparisons. Retain valid match results for performance comparisons. One match can contain more than one intentional death.");
        report.line("Show the count per 100 matches.");
        let mut csv = Vec::new();
        let points: Vec<_> = data.audits.iter().filter(|a| a.played > 0).map(|a| {
            let mean = a.excluded_death_resets as f64 * 100.0 / a.played as f64;
            csv.push(format!("{},{},{},{mean}", a.season, a.excluded_death_resets, a.played));
            report.line(format!("S{}: {} intentional spawnpoint deaths excluded / {} matches ({mean:.2} per 100)", a.season, a.excluded_death_resets, a.played));
            ResearchEstimate { x: f64::from(a.season), mean, low: mean, high: mean, n: a.played, players: 0 }
        }).collect();
        let y_max = points.iter().map(|p| p.mean).fold(1.0f64, f64::max) * 1.3;
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Intentional deaths excluded",
                x_desc: "Season",
                y_desc: "Deaths / 100 matches",
                x_range: f64::from(data.audits.first().map_or(1, |a| a.season)) - 0.5
                    ..f64::from(data.audits.last().map_or(1, |a| a.season)) + 0.5,
                y_range: 0.0..y_max,
                x_format: &|x: &f64| {
                    if (x.round() - x).abs() < 0.01 {
                        format!("S{x:.0}")
                    } else {
                        String::new()
                    }
                },
                y_format: &integer_tick,
                x_labels: data.audits.len() + 2,
                curves: vec![ResearchCurve {
                    label: "Intentional spawnpoint deaths".into(),
                    color: BASE_500,
                    points,
                }],
                reference: None,
            },
        )?;
        research_csv(
            &output.with_extension("csv"),
            "season,excluded_death_reset_events,ranked_matches,events_per_100_matches",
            csv,
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
