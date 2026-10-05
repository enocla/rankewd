use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct ForfeitCrossSeasonRating;
impl Analysis for ForfeitCrossSeasonRating {
    fn name(&self) -> &'static str {
        "forfeit-cross-season-rating"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input
            .output()
            .resolve("output/forfeit_cross_season_rating.png");
        let mut report = AnalysisReport::default();
        report.line("Elo percentile change between seasons");
        report.line("Compare players who return in the next adjacent season. Use the last 50 rated games and the next season first 50 games. Compare the Elo percentile in each game period. Each plotted change subtracts its comparison group mean.");
        report.line("The player population and placement games can change the percentile. A percentile change does not measure skill alone.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        let lookup: std::collections::HashMap<_, _> = data
            .profiles
            .iter()
            .map(|p| ((p.season, p.uuid.as_str()), p))
            .collect();
        let mut distributions: std::collections::HashMap<(u32, bool), Vec<f64>> =
            std::collections::HashMap::new();
        for p in &data.profiles {
            if p.total.games >= 100
                && let Some(x) = p.last.elo.mean()
            {
                distributions.entry((p.season, false)).or_default().push(x);
            }
            if p.total.games >= 50
                && let Some(x) = p.first.elo.mean()
            {
                distributions.entry((p.season, true)).or_default().push(x);
            }
        }
        for v in distributions.values_mut() {
            v.sort_unstable_by(f64::total_cmp);
        }
        let percentile = |season, first, value: f64| -> Option<f64> {
            let d = distributions.get(&(season, first))?;
            let lo = d.partition_point(|x| *x < value);
            let hi = d.partition_point(|x| *x <= value);
            Some((lo + hi) as f64 / (2 * d.len()) as f64)
        };
        let mut samples = Vec::new();
        for p in data.profiles.iter().filter(|p| p.total.games >= 100) {
            let Some(next) = lookup
                .get(&(p.season + 1, p.uuid.as_str()))
                .filter(|n| n.total.games >= 50)
            else {
                continue;
            };
            let value = p
                .last
                .elo
                .mean()
                .zip(next.first.elo.mean())
                .and_then(|(a, b)| {
                    percentile(next.season, true, b)
                        .zip(percentile(p.season, false, a))
                        .map(|(after, before)| after - before)
                });
            if let Some(y) = value {
                let rank = research_rank(p.last.elo.mean().map(|x| x as i32));
                samples.push(ResearchObservation {
                    uuid: p.uuid.clone(),
                    rank,
                    x: p.last.quit_rate(),
                    y,
                    stratum: format!(
                        "{}:{}:{}:{}:{}",
                        p.season,
                        rank,
                        (p.last.elo.mean().unwrap_or(0.0) / 200.0) as i32,
                        p.last.pace.mean().map(|x| (x / 300.0) as i32).unwrap_or(-1),
                        (p.last.delta.sum / 200.0) as i32
                    ),
                });
            }
        }
        let _ = percentile; // Pace graph uses the same paired-cohort construction.
        report.line(format!(
            "Paired measurable transitions: {} (previous last 50 / next first 50 rated games)",
            samples.len()
        ));
        if let Some((b, se)) = research_center(&mut samples) {
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
                title: "Elo percentile change between seasons",
                x_desc: "Your previous season forfeit rate",
                y_desc: "Percentile change − mean",
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
