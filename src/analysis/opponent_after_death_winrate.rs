use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use plotters::prelude::*;
use std::error::Error;

pub(super) struct OpponentAfterDeathWinrate;
impl Analysis for OpponentAfterDeathWinrate {
    fn name(&self) -> &'static str {
        "opponent-after-death-winrate"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input
            .output()
            .resolve("output/opponent_after_death_winrate.png");
        let root = canvas(&output)?;
        let labels = ["No event", "Death only", "Forfeit only", "Both events"];
        let mut chart = chart_builder(&root, "Your win rate after an opponent event")
            .build_cartesian_2d(-0.5f64..3.5f64, -0.5f64..5.5f64)?;
        style_mesh(
            &mut chart.configure_mesh(),
            "Opponent result after your death",
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
        report.line("Your win rate after an opponent event");
        report.line("Use the first ordinary death in matches without your forfeit. A later split is not necessary. Count opponent events strictly after your death and before the match ends. Each match enters one of four opponent result groups.");
        report.line("No event means no recorded death or forfeit. Count a draw as a match without a win. The CSV also gives all death matches as a separate reference. The figure uses only matches without your forfeit.");
        report.line("Future opponent events are unknown at time of death. These results do not show cause and effect. The 95% confidence intervals account for repeated players and shared matches. Cells with low counts have fewer than 100 matches or 20 players.");
        for c in data
            .opponent_after
            .iter()
            .filter(|c| c.key.prior_death == 2)
        {
            let rank = if c.key.rank == 7 {
                "All ratings"
            } else {
                RESEARCH_RANKS[c.key.rank as usize]
            };
            let group = match c.key.kind {
                0 => "All first deaths",
                1 => "Any later death",
                2 => "Any later forfeit",
                3 => "Neither",
                4 => "Death only",
                5 => "Forfeit only",
                6 => "Both",
                _ => continue,
            };
            csv.push(format!(
                "{rank},{group},{},{},{},{},{},{},{},{},{},{},{},{}",
                c.key.behavior,
                c.opportunities,
                c.selected,
                c.wins,
                c.selected_players,
                c.win_rate(),
                c.win_low,
                c.win_high,
                c.frequency(),
                c.frequency_low,
                c.frequency_high,
                c.opportunity_players
            ));
            let cohort = match c.key.behavior {
                0 => "all first deaths",
                1 => "no own forfeit",
                _ => "no own forfeit",
            };
            report.line(format!("{rank} / {cohort} / {group}: win {:.2}% [{:.2},{:.2}], wins={} / opportunities={}; group frequency {:.2}%",100.0*c.win_rate(),100.0*c.win_low,100.0*c.win_high,c.wins,c.selected,100.0*c.frequency()));
            if c.key.behavior != 1 || c.key.rank >= 6 || !(3..=6).contains(&c.key.kind) {
                continue;
            }
            let x = f64::from(c.key.kind - 3);
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
        }
        root.present()?;
        research_csv(
            &output.with_extension("csv"),
            "rank,group,behavior,first_deaths,observations,wins,players,mean,low_95,high_95,frequency,frequency_low_95,frequency_high_95,opportunity_players",
            csv,
        )?;
        report.line(format!("Chart: {}", output.display()));
        report.line(format!("Data: {}", output.with_extension("csv").display()));
        Ok(report)
    }
}
