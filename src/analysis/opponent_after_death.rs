use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use std::error::Error;

pub(super) struct OpponentAfterDeath;
impl Analysis for OpponentAfterDeath {
    fn name(&self) -> &'static str {
        "opponent-after-death"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/opponent_after_death.png");
        let mut report = AnalysisReport::default();
        report.line("Opponent results after your death");
        report.line("Use the first ordinary death in matches without your forfeit. A later split is not necessary. Exclude intentional spawnpoint deaths. Count opponent events strictly after your death.");
        report.line("Stop at the first completion, the first forfeit by either player, or the match end. An opponent can die and then forfeit. The two rates can overlap. A missing event record does not prove that the event did not occur.");
        report
            .line("The 95% confidence intervals account for repeated players and shared matches.");
        report.line(format!(
            "Ordinary death events recorded after a forfeit, excluded from opponent-event measurement: {}",
            data.audits.iter().map(|a| a.post_forfeit_deaths).sum::<u64>()
        ));
        let mut curves = Vec::new();
        let mut csv = Vec::new();
        for (behavior, kind, label) in [
            (1, 1, "Opponent dies after you"),
            (1, 2, "Opponent forfeits after you"),
        ] {
            let mut points = Vec::new();
            for c in data.opponent_after.iter().filter(|c| {
                c.key.prior_death == 2
                    && c.key.rank < 6
                    && c.key.behavior == behavior
                    && c.key.kind == kind
            }) {
                let x = f64::from(c.key.rank);
                csv.push(format!(
                    "{label},{x},{},{behavior},{},{},{},{},{},{},{},{}",
                    RESEARCH_RANKS[c.key.rank as usize],
                    c.opportunities,
                    c.selected,
                    c.wins,
                    c.opportunity_players,
                    c.selected_players,
                    c.frequency(),
                    c.frequency_low,
                    c.frequency_high
                ));
                report.line(format!("{} {label}: {:.2}% [{:.2},{:.2}], opponent events={} / first deaths={}, players={}",RESEARCH_RANKS[c.key.rank as usize],100.0*c.frequency(),100.0*c.frequency_low,100.0*c.frequency_high,c.selected,c.opportunities,c.opportunity_players));
                if c.opportunities >= 100 && c.opportunity_players >= 20 {
                    points.push(ResearchEstimate {
                        x,
                        mean: c.frequency(),
                        low: c.frequency_low,
                        high: c.frequency_high,
                        n: c.opportunities,
                        players: c.opportunity_players,
                    });
                }
            }
            curves.push(ResearchCurve {
                label: label.into(),
                color: SERIES_COLORS[curves.len()],
                points,
            });
        }
        research_csv(
            &output.with_extension("csv"),
            "series,x,rank,behavior,opportunities,event_count,wins,opportunity_players,event_players,mean,low_95,high_95",
            csv,
        )?;
        let y_max = (curves
            .iter()
            .flat_map(|c| c.points.iter().map(|p| p.high))
            .fold(0.05f64, f64::max)
            + 0.2)
            .min(1.2);
        draw_research_chart(
            &output,
            &ResearchChart {
                title: "Opponent results after your death",
                x_desc: "Your rank before the match",
                y_desc: "Opponent event rate",
                x_range: -0.4..5.5,
                y_range: 0.0..y_max,
                x_format: &|x| {
                    let i = x.round() as usize;
                    if (*x - i as f64).abs() < 0.01 && i < 6 {
                        RESEARCH_RANKS[i].into()
                    } else {
                        String::new()
                    }
                },
                y_format: &percent_tick,
                x_labels: 10,
                curves,
                reference: None,
            },
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
