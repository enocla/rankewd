use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{HOURS_PER_DAY, SeasonHours, load_season_hours};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER, RED_600,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend,
};
use plotters::element::DashedPathElement;
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "hour_entropy_season_";
/// Games a player needs before their hour-of-day entropy is stable enough to
/// bin. Below this, entropy mostly measures how few games a player has.
const MIN_GAMES_FOR_ENTROPY: u64 = 100;
/// Width of one entropy bin, in bits.
const ENTROPY_BIN_BITS: f64 = 0.25;
const X_AXIS_TITLE: &str = "Hour-of-day entropy (bits; 4.58 is even across 24h)";
const Y_AXIS_TITLE: &str = "Players";
const BAR_OPACITY: f64 = 0.85;
const LINE_WIDTH: u32 = 3;
/// Dash and gap lengths of the median marker.
const DASH_SIZE: u32 = 12;
const DASH_SPACING: u32 = 8;

pub(super) struct HourEntropy;

/// Entropy of a player's matches per UTC hour, counted into fixed-width bins.
/// Entropy is measured in bits, where 0 bits means every match fell in one UTC
/// hour and `log2(24) = 4.58` bits means an even spread across the whole day.
struct EntropyBins {
    season: u32,
    counts: Vec<u64>,
    players: usize,
    too_few_games: usize,
    median: f64,
    mean: f64,
    /// Lowest entropy reached, meaningful only when `players` is not zero.
    narrowest: f64,
    near_maximum: usize,
}

/// The entropy of a player whose matches are spread evenly over the day.
fn max_entropy() -> f64 {
    (HOURS_PER_DAY as f64).log2()
}

impl Analysis for HourEntropy {
    fn name(&self) -> &'static str {
        "hour-entropy"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let seasons = load_season_hours(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;

        let mut report = AnalysisReport::default();
        for season in &seasons {
            let bins = bin_entropies(season);
            if bins.players == 0 {
                report.line(format!(
                    "Season {}: no player played at least {MIN_GAMES_FOR_ENTROPY} matches",
                    season.season
                ));
                continue;
            }

            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", bins.season));
            draw_chart(&path, &bins)?;
            report.line(format!(
                "Season {}: {} of {} players played at least {MIN_GAMES_FOR_ENTROPY} matches; {} had fewer",
                bins.season,
                bins.players,
                season.players.len(),
                bins.too_few_games
            ));
            report.line(format!(
                "  Median entropy {:.2} bits and mean {:.2} bits, of a {:.2}-bit maximum across {HOURS_PER_DAY} UTC hours",
                bins.median,
                bins.mean,
                max_entropy()
            ));
            report.line(format!(
                "  The median is equivalent to playing {:.1} equally used hours; the narrowest player reached {:.2} bits, or {:.1} hours",
                2f64.powf(bins.median),
                bins.narrowest,
                2f64.powf(bins.narrowest)
            ));
            report.line(format!(
                "  {} players ({:.2}%) sit within {ENTROPY_BIN_BITS:.2} bits of the maximum",
                bins.near_maximum,
                percent_of(bins.near_maximum, bins.players)
            ));
            report.line(format!("  Hour-of-day entropy: {}", path.display()));
        }
        Ok(report)
    }
}

/// Bins each eligible player's hour-of-day entropy, measuring how evenly a
/// player spreads their matches over the day.
fn bin_entropies(season: &SeasonHours) -> EntropyBins {
    let bin_count = (max_entropy() / ENTROPY_BIN_BITS).ceil() as usize;
    let mut bins = EntropyBins {
        season: season.season,
        counts: vec![0; bin_count],
        players: 0,
        too_few_games: 0,
        median: 0.0,
        mean: 0.0,
        narrowest: f64::INFINITY,
        near_maximum: 0,
    };

    let mut entropies = Vec::new();
    for hours in season.players.values() {
        let total: u64 = hours.iter().sum();
        if total < MIN_GAMES_FOR_ENTROPY {
            bins.too_few_games += 1;
            continue;
        }

        let entropy = entropy_bits(hours, total);
        let bin = ((entropy / ENTROPY_BIN_BITS) as usize).min(bin_count - 1);
        bins.counts[bin] += 1;
        bins.players += 1;
        bins.narrowest = bins.narrowest.min(entropy);
        if max_entropy() - entropy <= ENTROPY_BIN_BITS {
            bins.near_maximum += 1;
        }
        entropies.push(entropy);
    }

    if !entropies.is_empty() {
        entropies.sort_by(f64::total_cmp);
        bins.median = percentile(&entropies, 0.5);
        bins.mean = entropies.iter().sum::<f64>() / entropies.len() as f64;
    }
    bins
}

/// Shannon entropy in bits of a player's matches over the 24 UTC hours. A
/// player whose matches all fall in one hour scores 0.
fn entropy_bits(hours: &[u64; HOURS_PER_DAY], total: u64) -> f64 {
    hours
        .iter()
        .filter(|&&matches| matches > 0)
        .map(|&matches| {
            let share = matches as f64 / total as f64;
            -share * share.log2()
        })
        .sum()
}

/// Linear-interpolated percentile of an already sorted, non-empty slice.
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let position = fraction * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = (position.ceil() as usize).min(sorted.len() - 1);
    let weight = position - lower as f64;
    sorted[lower] + (sorted[upper] - sorted[lower]) * weight
}

/// Share of `total` taken by `part`, in percent.
fn percent_of(part: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    part as f64 / total as f64 * 100.0
}

fn draw_chart(path: &Path, bins: &EntropyBins) -> Result<(), Box<dyn Error>> {
    if bins.players == 0 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no player entropies to plot").into(),
        );
    }
    create_parent(path)?;

    let max_count = bins.counts.iter().copied().max().unwrap_or(0).max(1);
    let x_end = bins.counts.len() as f64 * ENTROPY_BIN_BITS;
    let y_end = max_count + max_count / 10 + 1;
    let median = bins.median;

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!(
                "Season {}: Hour-of-Day Entropy (median {:.2} bits)",
                bins.season, bins.median
            ),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0f64..x_end, 0u64..y_end)?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(5)
        .y_labels(8)
        .x_label_formatter(&|bits| {
            if (bits - bits.round()).abs() < 0.01 {
                format!("{:.0}", bits.round())
            } else {
                String::new()
            }
        })
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(bins.counts.iter().enumerate().map(|(index, count)| {
            let left = index as f64 * ENTROPY_BIN_BITS;
            Rectangle::new(
                [(left, 0), (left + ENTROPY_BIN_BITS, *count)],
                BLUE_600.mix(BAR_OPACITY).filled(),
            )
        }))?
        .label("Players")
        .legend(|(x, y)| {
            Rectangle::new(
                [(x, y - 5), (x + LEGEND_WIDTH, y + 5)],
                BLUE_600.mix(BAR_OPACITY).filled(),
            )
        });
    chart
        .draw_series(std::iter::once(DashedPathElement::new(
            vec![(median, 0), (median, y_end)],
            DASH_SIZE,
            DASH_SPACING,
            RED_600.stroke_width(LINE_WIDTH),
        )))?
        .label("Median player entropy")
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + LEGEND_WIDTH, y)],
                RED_600.stroke_width(LINE_WIDTH),
            )
        });
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
