use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_800, BLUE_600, FONT_FAMILY, LEGEND_WIDTH, RED_600, TICK_LABEL_FONT_SIZE, canvas,
    chart_builder, draw_series_legend, legend_line, percent_tick, season_span, season_tick,
    style_mesh,
};
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/veteran_edge.png";
/// Largest pre-match Elo gap of a charted match, so the pair is evenly rated.
const MAX_GAP: i32 = 50;
const MIN_SEASON_MATCHES: u64 = 200;

pub(super) struct VeteranEdge;

#[derive(Clone, Copy, Default)]
struct Tally {
    matches: u64,
    veteran_wins: u64,
    expected: f64,
}

impl Analysis for VeteranEdge {
    fn name(&self) -> &'static str {
        "veteran-edge"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        // First selected season each player appears in.
        let mut debut: HashMap<u128, u32> = HashMap::new();
        for duel in &duels.duels {
            for side in &duel.sides {
                debut
                    .entry(side.uuid)
                    .and_modify(|season| *season = (*season).min(duel.season))
                    .or_insert(duel.season);
            }
        }

        let mut tallies: BTreeMap<u32, Tally> = BTreeMap::new();
        for duel in &duels.duels {
            let (Some((first, second)), Some(winner)) = (duel.elos(), duel.winner) else {
                continue;
            };
            if (first - second).abs() > MAX_GAP {
                continue;
            }
            let veteran: Vec<bool> = duel
                .sides
                .iter()
                .map(|side| debut[&side.uuid] < duel.season)
                .collect();
            if veteran[0] == veteran[1] {
                continue;
            }
            let side = usize::from(veteran[1]);
            let tally = tallies.entry(duel.season).or_default();
            tally.matches += 1;
            tally.veteran_wins += u64::from(winner == side);
            tally.expected += duel.expected_score(side).unwrap_or(0.5);
        }
        tallies.retain(|_, tally| tally.matches >= MIN_SEASON_MATCHES);
        if tallies.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no season had enough veteran-versus-newcomer matches; select several seasons",
            )
            .into());
        }

        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        let seasons: Vec<u32> = tallies.keys().copied().collect();
        draw_chart(&output, &seasons, &tallies)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: returning player versus season debutant, pre-match gap at most {MAX_GAP}",
            season_span(&seasons)
        ));
        for (season, tally) in &tallies {
            report.line(format!(
                "  S{season}: veteran won {:.1}% (Elo expects {:.1}%) of {} matches",
                100.0 * tally.veteran_wins as f64 / tally.matches as f64,
                100.0 * tally.expected / tally.matches as f64,
                tally.matches
            ));
        }
        report.line(format!("  Veteran edge: {}", output.display()));
        Ok(report)
    }
}

fn draw_chart(
    path: &Path,
    seasons: &[u32],
    tallies: &BTreeMap<u32, Tally>,
) -> Result<(), Box<dyn Error>> {
    let rates: Vec<f64> = tallies
        .values()
        .map(|tally| tally.veteran_wins as f64 / tally.matches as f64)
        .collect();
    let top = rates.iter().copied().fold(0.5, f64::max) + 0.04;
    let bottom = rates.iter().copied().fold(0.5, f64::min) - 0.04;
    let last = seasons.len() as f64 - 0.5;

    let root = canvas(path)?;
    let mut chart = chart_builder(
        &root,
        &format!(
            "{}: Returning Players vs Equal-Elo Debutants",
            season_span(seasons)
        ),
    )
    .build_cartesian_2d(-0.5f64..last, bottom..top)?;
    style_mesh(
        &mut chart.configure_mesh(),
        "Season",
        "Returning player win rate",
    )
    .disable_x_mesh()
    .x_labels(seasons.len() * 2 + 1)
    .x_label_formatter(&|position| season_tick(seasons, *position))
    .y_label_formatter(&percent_tick)
    .draw()?;

    chart
        .draw_series(rates.iter().enumerate().map(|(index, rate)| {
            let x = index as f64;
            let color = if *rate >= 0.5 { BLUE_600 } else { RED_600 };
            Rectangle::new(
                [(x - 0.32, 0.5), (x + 0.32, *rate)],
                color.mix(0.85).filled(),
            )
        }))?
        .label("Returning player's actual win rate")
        .legend(|(x, y)| {
            Rectangle::new([(x, y - 6), (x + LEGEND_WIDTH, y + 6)], BLUE_600.filled())
        });
    let above = Pos::new(HPos::Center, VPos::Bottom);
    let below = Pos::new(HPos::Center, VPos::Top);
    chart.draw_series(rates.iter().enumerate().map(|(index, rate)| {
        let offset = if *rate >= 0.5 { 0.004 } else { -0.004 };
        Text::new(
            format!("{:.1}%", rate * 100.0),
            (index as f64, rate + offset),
            (FONT_FAMILY, TICK_LABEL_FONT_SIZE - 8)
                .into_font()
                .color(&BASE_800)
                .pos(if *rate >= 0.5 { above } else { below }),
        )
    }))?;
    chart
        .draw_series(DashedLineSeries::new(
            [(-0.5, 0.5), (last, 0.5)],
            12,
            8,
            BASE_800.stroke_width(2),
        ))?
        .label("Coin flip (Elo expects ≈50%)")
        .legend(legend_line(BASE_800));
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
