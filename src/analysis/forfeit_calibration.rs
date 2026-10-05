use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;
pub(super) struct ForfeitCalibration;
impl Analysis for ForfeitCalibration {
    fn name(&self) -> &'static str {
        "forfeit-calibration"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/forfeit_calibration.png");
        let mut report = AnalysisReport::default();
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        for (all, predicted, label) in [
            (true, false, "Score: all matches"),
            (false, false, "Score: completed matches"),
            (true, true, "Prediction: all matches"),
            (false, true, "Prediction: completed matches"),
        ] {
            let points: Vec<_> = data
                .calibration
                .iter()
                .filter(|c| c.season == 0 && c.all == all && c.n >= 200)
                .map(|c| {
                    let x = (f64::from(c.gap) + 0.5) * 50.0;
                    let y = if predicted { c.expected } else { c.score } / c.n as f64;
                    csv.push(format!("{label},{x},{y},{y},{y},{},0", c.n));
                    report.line(format!(
                        "{label} gap {x:.0}: {:.2}% over {} games",
                        y * 100.0,
                        c.n
                    ));
                    ResearchEstimate {
                        x,
                        mean: y,
                        low: y,
                        high: y,
                        n: c.n,
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
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Elo prediction and match score",
                x_desc: "Elo difference before the match",
                y_desc: "Higher rated player score",
                x_range: 0.0..500.0,
                y_range: 0.4..1.2,
                x_format: &integer_tick,
                y_format: &percent_tick,
                x_labels: 6,
                curves,
                reference: None,
            },
        )?;
        report.line("Use the conventional 400-point Elo formula as a prediction reference. This formula is not a verified implementation of the game. A draw has a score of 0.5. Compare predictions with actual scores for all matches and completed matches.");
        report.line("Use this figure to assess the score difference plots.");
        report.line(format!("Chart: {}", output.display()));
        Ok(report)
    }
}
