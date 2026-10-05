use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct RecoveryStage;
impl Analysis for RecoveryStage {
    fn name(&self) -> &'static str {
        "recovery-stage"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/recovery_stage.png");
        let mut report = AnalysisReport::default();
        report.line("Win rate by split at death");
        report.line("Use the first ordinary death in each player match. Include the match if the player does not forfeit. A later split is not necessary. Ignore world resets and exclude intentional spawnpoint deaths.");
        report.line("The 95% confidence intervals account for repeated players and shared matches. A missing forfeit record does not prove that the player was active. These results do not show cause and effect.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        let (kind, behavior, label) = (0, 1, "Continue play after death");
        let points:Vec<_>=data.recovery.iter().filter(|c|c.key.season==0 && c.key.rank==7 && c.key.view==1 && c.key.kind==kind && c.key.behavior==behavior && c.n>=100 && c.players>=20)
  .map(|c| { let x=f64::from(c.key.bin); report.line(format!("  {label} x={x:.1}: {:.2}% [{:.2},{:.2}], n={}, players={}, equal player weight {:.2}%",100.0*c.rate(),100.0*c.low,100.0*c.high,c.n,c.players,100.0*c.player_mean));
   csv.push(format!("{label},{x},{},{},{},{},{}",c.rate(),c.low,c.high,c.n,c.players));
   ResearchEstimate{x,mean:c.rate(),low:c.low,high:c.high,n:c.n,players:c.players} }).collect();
        curves.push(ResearchCurve {
            label: label.to_owned(),
            color: SERIES_COLORS[curves.len()],
            points,
        });
        research_csv(
            &output.with_extension("csv"),
            "series,x,mean,low_95,high_95,observations,players",
            csv,
        )?;
        let y_range = 0.0..(curves
            .iter()
            .flat_map(|c: &ResearchCurve| c.points.iter().map(|p| p.high))
            .fold(0.05f64, f64::max)
            + 0.18)
            .min(1.2);
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Win rate by split at death",
                x_desc: "Latest split reached before death",
                y_desc: "Win rate",
                x_range: -0.4..6.5,
                y_range,
                x_format: &|x: &f64| {
                    let i = x.round() as usize;
                    [
                        "Start",
                        "Nether",
                        "Bastion",
                        "Fortress",
                        "Blind",
                        "Stronghold",
                        "End",
                    ]
                    .get(i)
                    .unwrap_or(&"")
                    .to_string()
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
