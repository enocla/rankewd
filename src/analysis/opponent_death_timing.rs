use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use plotters::prelude::*;
use std::error::Error;

pub(super) struct OpponentDeathTiming;
impl Analysis for OpponentDeathTiming {
    fn name(&self) -> &'static str {
        "opponent-death-timing"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/opponent_death_timing.png");
        let root = canvas(&output)?;
        let labels = [
            "No death",
            "Before yours",
            "After yours",
            "Before and after",
        ];
        let mut chart = chart_builder(&root, "Win rate and opponent death time")
            .build_cartesian_2d(-0.5f64..3.5f64, -0.5f64..5.5f64)?;
        style_mesh(
            &mut chart.configure_mesh(),
            "Opponent death time relative to yours",
            "Your rank before the match",
        )
        .x_labels(8)
        .y_labels(10)
        .x_label_formatter(&|x| labels.get(x.round() as usize).unwrap_or(&"").to_string())
        .y_label_formatter(&|y| {
            RESEARCH_RANKS
                .get(y.round() as usize)
                .unwrap_or(&"")
                .to_string()
        })
        .draw()?;
        let mut csv = Vec::new();
        let mut report = AnalysisReport::default();
        report.line("Win rate and opponent death time");
        report.line("Use the first ordinary death in matches without your forfeit. A later split is not necessary. Compare opponent deaths before and after your death. Deaths at the same time count as neither earlier nor later.");
        report.line("Stop at the first completion, the first forfeit by either player, or the match end. Include opponent forfeits and draws. An opponent can die before your death and again after your death. Exclude intentional spawnpoint deaths.");
        report.line("A missing forfeit record does not prove active play. Future opponent events are unknown at time of death. These results do not show cause and effect. The 95% confidence intervals account for repeated players and shared matches.");
        report.line("Cells with low counts have fewer than 100 matches or 20 players.");
        for c in data.opponent_after.iter().filter(|c| {
            c.key.prior_death < 2
                && c.key.behavior == 1
                && c.key.rank < 6
                && [1, 7].contains(&c.key.kind)
        }) {
            let later = u8::from(c.key.kind == 1);
            let bin = c.key.prior_death + 2 * later;
            let x = f64::from(bin);
            let y = f64::from(c.key.rank);
            let enough = c.selected >= 100 && c.selected_players >= 20;
            let fill = if enough {
                heat_color(c.win_rate())
            } else {
                BASE_150
            };
            chart.draw_series(std::iter::once(Rectangle::new(
                [(x - 0.49, y - 0.49), (x + 0.49, y + 0.49)],
                fill.filled(),
            )))?;
            chart.draw_series(std::iter::once(Text::new(
                if enough {
                    format!("{:.1}%", 100.0 * c.win_rate())
                } else {
                    "Low counts".into()
                },
                (x - 0.32, y + 0.05),
                (FONT_FAMILY, 52).into_font().color(&text_on(fill)),
            )))?;
            if enough {
                chart.draw_series(std::iter::once(Text::new(
                    format!("{:.1}–{:.1}", 100.0 * c.win_low, 100.0 * c.win_high),
                    (x - 0.36, y - 0.22),
                    (FONT_FAMILY, 36).into_font().color(&text_on(fill)),
                )))?;
            }
            csv.push(format!(
                "{},{},{},{},{},{},{},{},{},{}",
                RESEARCH_RANKS[c.key.rank as usize],
                labels[bin as usize],
                c.opportunities,
                c.selected,
                c.wins,
                c.selected_players,
                c.win_rate(),
                c.win_low,
                c.win_high,
                enough
            ));
            report.line(format!(
                "{} / {}: {:.2}% [{:.2},{:.2}], wins={} / matches={}, players={}",
                RESEARCH_RANKS[c.key.rank as usize],
                labels[bin as usize],
                100.0 * c.win_rate(),
                100.0 * c.win_low,
                100.0 * c.win_high,
                c.wins,
                c.selected,
                c.selected_players
            ));
        }
        root.present()?;
        research_csv(
            &output.with_extension("csv"),
            "rank,timing,prior_status_opportunities,observations,wins,players,mean,low_95,high_95,plotted",
            csv,
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
