use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct RecoveryEfficiency;
impl Analysis for RecoveryEfficiency {
    fn name(&self) -> &'static str {
        "recovery-efficiency"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/recovery_efficiency.png");
        let mut report = AnalysisReport::default();
        report.line("Elo per hour after death");
        report.line("Start at the first ordinary death. Compare a forfeit after death with continued play after death. Continued play means no recorded forfeit by that player. A later split is not necessary.");
        report.line("Ignore world resets. Compare groups with the same season, Elo, opponent Elo difference, split, and split lead. Each group needs at least 20 observations for each action. Both actions use the smaller group count as their weight.");
        report.line("The 10-game period includes this match and nine later rated games. Time includes the remainder of this match and the later matches. Queue time and penalties are unknown. Both actions need complete data for the period.");
        report.line("These results do not show cause and effect.");
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        type Totals = (u64, f64, f64);
        let mut cells: std::collections::HashMap<(u32, u8, i32, u8, u8), Totals> =
            std::collections::HashMap::new();
        for a in &data.efficiency {
            for h in 0..3 {
                let (n, delta, time) = match h {
                    0 => (a.immediate.n, a.immediate.sum, a.remaining_minutes),
                    1 => (a.horizons, a.next_delta, a.next_minutes),
                    _ => (a.session_horizons, a.session_delta, a.session_minutes),
                };
                let t = cells
                    .entry((a.season, a.rank, a.stratum, a.action, h))
                    .or_default();
                t.0 += n;
                t.1 += delta;
                t.2 += time;
            }
        }
        for (action, horizon, label) in [
            (0, 1, "Forfeit: 10 games"),
            (1, 1, "Continue: 10 games"),
            (0, 2, "Forfeit: same session"),
            (1, 2, "Continue: same session"),
        ] {
            let mut points = Vec::new();
            for rank in 0..6 {
                let mut numerator = 0.0;
                let mut denominator = 0.0;
                let mut clusters: std::collections::HashMap<&str, (f64, f64)> =
                    std::collections::HashMap::new();
                let mut observations = 0u64;
                let mut state_count = 0u64;
                let mut scales = std::collections::HashMap::new();
                for ((season, r, stratum, a, h), own) in &cells {
                    if *r != rank || *a != action || *h != horizon || own.0 < 20 {
                        continue;
                    }
                    let other = cells
                        .get(&(*season, *r, *stratum, 1 - action, horizon))
                        .copied()
                        .unwrap_or_default();
                    if other.0 < 20 {
                        continue;
                    }
                    state_count += 1;
                    let scale = own.0.min(other.0) as f64 / own.0 as f64;
                    scales.insert((*season, *stratum), scale);
                    numerator += scale * own.1 * 60.0;
                    denominator += scale * own.2;
                    observations += own.0;
                }
                for row in data
                    .efficiency
                    .iter()
                    .filter(|x| x.rank == rank && x.action == action)
                {
                    let Some(scale) = scales.get(&(row.season, row.stratum)) else {
                        continue;
                    };
                    let (n, delta, time) = match horizon {
                        0 => (row.immediate.n, row.immediate.sum, row.remaining_minutes),
                        1 => (row.horizons, row.next_delta, row.next_minutes),
                        _ => (row.session_horizons, row.session_delta, row.session_minutes),
                    };
                    if n == 0 {
                        continue;
                    }
                    let pair = clusters.entry(&row.uuid).or_default();
                    pair.0 += scale * delta * 60.0;
                    pair.1 += scale * time;
                }
                if denominator <= 0.0 || clusters.len() < 20 {
                    continue;
                }
                let mean = numerator / denominator;
                let g = clusters.len();
                let half = 1.96
                    * (clusters
                        .values()
                        .map(|(num, den)| (num - mean * den).powi(2))
                        .sum::<f64>()
                        * g as f64
                        / (g - 1) as f64)
                        .sqrt()
                    / denominator;
                let x = f64::from(rank);
                let p = ResearchEstimate {
                    x,
                    mean,
                    low: mean - half,
                    high: mean + half,
                    n: observations,
                    players: g as u64,
                };
                report.line(format!("  {} {label}: {:.3} [{:.3},{:.3}], opportunities={}, players={}, matched states={state_count}",RESEARCH_RANKS[rank as usize],p.mean,p.low,p.high,p.n,p.players));
                csv.push(format!(
                    "{label},{},{},{},{},{},{}",
                    p.x, p.mean, p.low, p.high, p.n, p.players
                ));
                points.push(p);
            }
            curves.push(ResearchCurve {
                label: label.into(),
                color: SERIES_COLORS[curves.len()],
                points,
            });
        }
        report.line(format!("Death observations: {}; complete 10-game periods: {}; complete same-session periods: {}",data.efficiency.iter().map(|a|a.immediate.n).sum::<u64>(),data.efficiency.iter().map(|a|a.horizons).sum::<u64>(),data.efficiency.iter().map(|a|a.session_horizons).sum::<u64>()));
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
                title: "Elo per hour after death",
                x_desc: "Rank before the match",
                y_desc: "Elo gain per game hour",
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
                y_format: &integer_tick,
                x_labels: 10,
                curves,
                reference: Some((0.0, "No Elo gain")),
            },
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
