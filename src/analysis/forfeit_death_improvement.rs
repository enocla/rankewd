use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct ForfeitDeathImprovement;
impl Analysis for ForfeitDeathImprovement {
    fn name(&self) -> &'static str {
        "forfeit-death-improvement"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input
            .output()
            .resolve("output/forfeit_death_improvement.png");
        let mut report = AnalysisReport::default();
        report.line("Forfeit rate and later death rate");
        report.line("The earlier period contains the last 50 rated games before season day 30, 60, or 90. The later period contains the next 30 days. Comparison groups share the season, day, rank, earlier Elo, completion time, and earlier Elo gain. Each plotted outcome subtracts its comparison group mean.");
        report.line("The 95% confidence intervals account for repeated players. Only records with enough later data enter the comparison. These results do not show cause and effect.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        let mut samples = Vec::new();
        report.line(format!(
            "Records with enough earlier games: {}; no later rated games: {}",
            data.windows.len(),
            data.windows.iter().filter(|w| w.after.games == 0).count()
        ));
        for w in data
            .windows
            .iter()
            .filter(|w| w.after.games >= 10 && w.season != 1)
        {
            if let Some(y) = Some(
                w.before.deaths as f64 / w.before.games as f64
                    - w.after.deaths as f64 / w.after.games as f64,
            ) {
                let pace_band = w
                    .before
                    .pace
                    .mean()
                    .map(|x| (x / 300.0) as i32)
                    .unwrap_or(-1);
                samples.push(ResearchObservation {
                    uuid: w.uuid.clone(),
                    rank: w.rank,
                    x: w.before.quit_rate(),
                    y,
                    stratum: format!(
                        "{}:{}:{}:{}:{}:{}",
                        w.season,
                        w.day,
                        w.rank,
                        (w.before.elo.mean().unwrap_or(0.0) / 200.0) as i32,
                        pace_band,
                        (w.before.delta.sum / 200.0) as i32
                    ),
                });
            }
        }
        report.line(format!(
            "Records with enough later games and measured outcomes: {}",
            samples.len()
        ));
        let slope = research_center(&mut samples);
        if let Some((b, se)) = slope {
            report.line(format!(
                "Change for a 10-point increase in the earlier rate: {:.4} [95% {:.4},{:.4}]",
                b * 0.1,
                (b - 1.96 * se) * 0.1,
                (b + 1.96 * se) * 0.1
            ));
        }
        report.line(format!("Comparable player records: {}", samples.len()));
        for rank in 0..6 {
            let points = research_estimates(&samples, rank);
            let label = RESEARCH_RANKS[rank as usize];
            for p in &points {
                csv.push(format!(
                    "{label},{},{},{},{},{},{}",
                    p.x, p.mean, p.low, p.high, p.n, p.players
                ));
                report.line(format!(
                    "  {label} earlier rate {:.0}%: {:.4} [{:.4},{:.4}], records={}, players={}",
                    p.x * 100.0,
                    p.mean,
                    p.low,
                    p.high,
                    p.n,
                    p.players
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
        let y_range = value_range(
            curves
                .iter()
                .flat_map(|c: &ResearchCurve| c.points.iter().flat_map(|p| [p.low, p.high])),
            0.08,
            0.30,
        );
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Forfeit rate and later death rate",
                x_desc: "Your earlier forfeit rate",
                y_desc: "Death rate decrease − mean",
                x_range: 0.0..1.0,
                y_range,
                x_format: &percent_tick,
                y_format: &percent_tick,
                x_labels: 6,
                curves,
                reference: Some((0.0, "Comparison group mean")),
            },
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
