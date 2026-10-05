use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct ForfeitRetention;
impl Analysis for ForfeitRetention {
    fn name(&self) -> &'static str {
        "forfeit-retention"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/forfeit_retention.png");
        let mut report = AnalysisReport::default();
        report.line("Forfeit rate and next season return");
        report.line("Measure the forfeit rate in the last 50 rated games. Require at least 100 games in that season. A return means at least one rated match in the next season. Compare adjacent seasons only.");
        report.line("Do not treat a missing season as a failure to return. Season 12 contains only part of the season.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        let lookup: std::collections::HashMap<_, _> = data
            .profiles
            .iter()
            .map(|p| ((p.season, p.uuid.as_str()), p))
            .collect();
        let seasons: std::collections::HashSet<_> = data.audits.iter().map(|a| a.season).collect();
        let mut samples = Vec::new();
        for p in data
            .profiles
            .iter()
            .filter(|p| p.total.games >= 100 && seasons.contains(&(p.season + 1)))
        {
            let returned = lookup.contains_key(&(p.season + 1, p.uuid.as_str()));
            samples.push(ResearchObservation {
                uuid: p.uuid.clone(),
                rank: research_rank(p.last.elo.mean().map(|x| x as i32)),
                x: p.last.quit_rate(),
                y: f64::from(returned),
                stratum: format!("{}", p.season),
            });
        }
        report.line(format!(
            "Eligible transitions: {} (adjacent selected seasons only)",
            samples.len()
        ));
        for rank in 0..6 {
            let points = research_estimates(&samples, rank);
            let label = RESEARCH_RANKS[rank as usize];
            for p in &points {
                csv.push(format!(
                    "{label},{},{},{},{},{},{}",
                    p.x, p.mean, p.low, p.high, p.n, p.players
                ));
            }
            curves.push(ResearchCurve {
                label: label.into(),
                color: SERIES_COLORS[rank as usize],
                points,
            });
        }
        research_csv(
            &output.with_extension("csv"),
            "series,x,mean,low_95,high_95,observations,players",
            csv,
        )?;
        let y_range = 0.0..1.25;
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Forfeit rate and next season return",
                x_desc: "Your previous season forfeit rate",
                y_desc: "Next season return rate",
                x_range: 0.0..1.0,
                y_range,
                x_format: &percent_tick,
                y_format: &percent_tick,
                x_labels: 6,
                curves,
                reference: None,
            },
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
