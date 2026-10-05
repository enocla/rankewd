use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct ForfeitWinrate;
impl Analysis for ForfeitWinrate {
    fn name(&self) -> &'static str {
        "forfeit-winrate"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/forfeit_winrate.png");
        let mut report = AnalysisReport::default();
        report.line("Forfeit rate and win rate");
        report.line("Each player has one record per season and the same weight. Each record needs at least 100 rated games. Mean Elo determines the rank. Each plot group needs at least 20 players.");
        report.line("The 95% confidence intervals account for repeated players. These results show associations. They do not show cause and effect.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        let mut samples = Vec::new();
        for p in data.profiles.iter().filter(|p| p.total.games >= 100) {
            if let Some(y) = Some(p.total.wins as f64 / p.total.games as f64) {
                samples.push(ResearchObservation {
                    uuid: p.uuid.clone(),
                    rank: p.rank,
                    x: p.total.quit_rate(),
                    y,
                    stratum: format!("{}:{}", p.season, p.rank),
                });
            }
        }
        report.line(format!("Eligible player-seasons: {}", samples.len()));
        for rank in 0..6 {
            let points = research_estimates(&samples, rank);
            let label = RESEARCH_RANKS[rank as usize];
            for p in &points {
                csv.push(format!(
                    "{label},{},{},{},{},{},{}",
                    p.x, p.mean, p.low, p.high, p.n, p.players
                ));
                report.line(format!(
                    "  {label} forfeit {:.0}%: {:.4} [{:.4},{:.4}], player-seasons={}, players={}",
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
                title: "Forfeit rate and win rate",
                x_desc: "Your forfeit rate",
                y_desc: "Win rate",
                x_range: 0.0..0.9,
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
