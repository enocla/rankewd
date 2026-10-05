use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::*;
use crate::plot::*;
use plotters::prelude::*;
use std::error::Error;
pub(super) struct DeathRankDeficit;
impl Analysis for DeathRankDeficit {
    fn name(&self) -> &'static str {
        "death-rank-deficit"
    }
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = load_research(input)?;
        let output = input.output().resolve("output/death_rank_deficit.png");
        let root = canvas(&output)?;
        let mut chart = chart_builder(&root, "Win rate by rank and split lead")
            .build_cartesian_2d(-2.5f64..2.5f64, -0.5f64..5.5f64)?;
        style_mesh(
            &mut chart.configure_mesh(),
            "Opponent split lead at time of death",
            "Rank before the match",
        )
        .x_labels(6)
        .y_labels(10)
        .x_label_formatter(&|x| match x.round() as i32 {
            -2 => "-2+".into(),
            2 => "2+".into(),
            i => format!("{i}"),
        })
        .y_label_formatter(&|y| {
            RESEARCH_RANKS
                .get(y.round() as usize)
                .unwrap_or(&"")
                .to_string()
        })
        .draw()?;
        let mut report = AnalysisReport::default();
        let mut csv = Vec::new();
        for c in data.recovery.iter().filter(|c| {
            c.key.season == 0
                && c.key.rank < 6
                && c.key.view == 2
                && c.key.kind == 0
                && c.key.behavior == 1
        }) {
            let x = f64::from(c.key.bin);
            let y = f64::from(c.key.rank);
            let enough = c.n >= 100 && c.players >= 20;
            let fill = if enough {
                heat_color(c.rate() / 0.5)
            } else {
                BASE_150
            };
            chart.draw_series(std::iter::once(Rectangle::new(
                [(x - 0.49, y - 0.49), (x + 0.49, y + 0.49)],
                fill.filled(),
            )))?;
            let label = if enough {
                format!("{:.1}%", c.rate() * 100.0)
            } else {
                "Low counts".into()
            };
            chart.draw_series(std::iter::once(Text::new(
                label,
                (x - 0.34, y + 0.05),
                (FONT_FAMILY, 52).into_font().color(&text_on(fill)),
            )))?;
            if enough {
                chart.draw_series(std::iter::once(Text::new(
                    format!("{:.1}-{:.1}", c.low * 100.0, c.high * 100.0),
                    (x - 0.40, y - 0.22),
                    (FONT_FAMILY, 36).into_font().color(&text_on(fill)),
                )))?;
            }
            csv.push(format!(
                "{},{},{},{},{},{},{}",
                RESEARCH_RANKS[c.key.rank as usize],
                x,
                c.rate(),
                c.low,
                c.high,
                c.n,
                c.players
            ));
            report.line(format!(
                "{} deficit {}: {:.2}% [{:.2},{:.2}], opportunities={}, players={}",
                RESEARCH_RANKS[c.key.rank as usize],
                c.key.bin,
                c.rate() * 100.0,
                c.low * 100.0,
                c.high * 100.0,
                c.n,
                c.players
            ));
        }
        root.present()?;
        research_csv(
            &output.with_extension("csv"),
            "rank,opponent_splits_ahead,mean,low_95,high_95,observations,players",
            csv,
        )?;
        report.line("Use the first ordinary death in matches without your forfeit. A later split is not necessary. Ignore world resets. Include draws.");
        report.line("A negative split lead means that you are ahead. The outer groups combine leads of two or more splits. Each cell shows the win rate and its 95% confidence interval. Cells with low counts have fewer than 100 matches or 20 players.");
        report.line("A split lead does not measure inventory loss. These results do not show cause and effect.");
        report.line(format!("Chart: {}", output.display()));
        Ok(report)
    }
}
