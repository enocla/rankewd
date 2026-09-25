use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{PlayerSeason, PlayerSeasons, elo_range};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER, RED_600,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend, padded_bounds,
};
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "name_underscores_vs_elo_season_";
/// Players a group needs before its box describes more than a handful of
/// individuals.
const MIN_GROUP_SIZE: usize = 20;
/// Half-widths of the box and of a whisker cap, in underscore-count units.
const BOX_HALF_WIDTH: f64 = 0.3;
const CAP_HALF_WIDTH: f64 = 0.15;
const LINE_WIDTH: u32 = 2;
const MEDIAN_WIDTH: u32 = 4;
/// The box is this opaque, so overlapping whiskers stay visible through it.
const BOX_OPACITY: f64 = 0.45;
const X_AXIS_TITLE: &str = "Underscores in player name";
const Y_AXIS_TITLE: &str = "Final Elo in season";

pub(super) struct NameUnderscoresVersusElo;

/// One box: the Elo distribution of every player whose name holds the same
/// number of underscores.
struct EloGroup {
    underscores: i64,
    player_count: usize,
    low: f64,
    lower_quartile: f64,
    median: f64,
    upper_quartile: f64,
    high: f64,
    outliers: usize,
}

#[derive(Default)]
struct GroupedElos {
    groups: Vec<EloGroup>,
    rated_players: usize,
    dropped_groups: usize,
    dropped_players: usize,
}

impl Analysis for NameUnderscoresVersusElo {
    fn name(&self) -> &'static str {
        "name-underscores-versus-elo"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load_summaries(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;

        let mut report = AnalysisReport::default();
        for season in data.seasons()? {
            let grouped = group_by_underscores(&season.players);
            if grouped.groups.is_empty() {
                report.line(format!(
                    "Season {}: no underscore group reached {MIN_GROUP_SIZE} rated players",
                    season.season
                ));
                continue;
            }

            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &grouped)?;
            report.line(format!(
                "Season {}: {} rated players in {} underscore groups{}",
                season.season,
                grouped.rated_players,
                grouped.groups.len(),
                exclusion_note(&grouped)
            ));
            report.line(format!("  Median Elo: {}", median_summary(&grouped.groups)));
            report.line(format!("  Name underscores vs Elo: {}", path.display()));
        }
        Ok(report)
    }
}

fn exclusion_note(grouped: &GroupedElos) -> String {
    if grouped.dropped_groups == 0 {
        return String::new();
    }
    format!(
        "; {} groups below {MIN_GROUP_SIZE} players skipped, covering {} players",
        grouped.dropped_groups, grouped.dropped_players
    )
}

fn median_summary(groups: &[EloGroup]) -> String {
    groups
        .iter()
        .map(|group| {
            format!(
                "{}: {:.0} ({} players, {} outliers)",
                group.underscores, group.median, group.player_count, group.outliers
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Buckets rated players by the underscores in the name recorded alongside their
/// final Elo, then summarizes each bucket as a five-number summary.
fn group_by_underscores(players: &[&PlayerSeason]) -> GroupedElos {
    let mut by_underscores: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut grouped = GroupedElos::default();

    for player in players {
        let (Some(elo), Some(underscores)) = (player.latest_elo, player.latest_underscore_count)
        else {
            continue;
        };
        grouped.rated_players += 1;
        by_underscores
            .entry(i64::try_from(underscores).unwrap_or(i64::MAX))
            .or_default()
            .push(elo);
    }

    for (underscores, mut elos) in by_underscores {
        if elos.len() < MIN_GROUP_SIZE {
            grouped.dropped_groups += 1;
            grouped.dropped_players += elos.len();
            continue;
        }
        elos.sort_unstable();
        let lower_quartile = percentile(&elos, 0.25);
        let median = percentile(&elos, 0.5);
        let upper_quartile = percentile(&elos, 0.75);

        // Tukey's 1.5xIQR rule: each whisker reaches the furthest value still
        // inside its fence, and values past the fences are counted as outliers.
        let spread = upper_quartile - lower_quartile;
        let low_fence = lower_quartile - 1.5 * spread;
        let high_fence = upper_quartile + 1.5 * spread;
        let low = elos
            .iter()
            .copied()
            .map(|elo| elo as f64)
            .find(|elo| *elo >= low_fence)
            .unwrap_or(elos[0] as f64);
        let high = elos
            .iter()
            .copied()
            .rev()
            .map(|elo| elo as f64)
            .find(|elo| *elo <= high_fence)
            .unwrap_or(elos[elos.len() - 1] as f64);
        let outliers = elos
            .iter()
            .filter(|elo| {
                let elo = **elo as f64;
                elo < low_fence || elo > high_fence
            })
            .count();

        grouped.groups.push(EloGroup {
            underscores,
            player_count: elos.len(),
            low,
            lower_quartile,
            median,
            upper_quartile,
            high,
            outliers,
        });
    }

    grouped
}

/// Linear-interpolated percentile of an already sorted, non-empty slice.
fn percentile(sorted: &[i64], fraction: f64) -> f64 {
    let position = fraction * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = (position.ceil() as usize).min(sorted.len() - 1);
    let weight = position - lower as f64;
    sorted[lower] as f64 + (sorted[upper] as f64 - sorted[lower] as f64) * weight
}

fn draw_chart(path: &Path, season: u32, grouped: &GroupedElos) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let Some(last) = grouped.groups.last() else {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no underscore groups to plot").into(),
        );
    };
    let max_underscores = last.underscores as f64;
    let (min_elo, max_elo) = elo_range(
        grouped
            .groups
            .iter()
            .flat_map(|group| [group.low.round() as i64, group.high.round() as i64]),
    )?;
    let (y_min, y_max) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    // The x range reaches one whole unit past the data on each side so the
    // outermost boxes fit; only counts that have a group are labelled.
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Final Elo by Name Underscores"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(-1.0f64..(max_underscores + 1.0), y_min as f64..y_max as f64)?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(6)
        .y_labels(8)
        .y_label_formatter(&|elo| format!("{elo:.0}"))
        .x_label_formatter(&|count| {
            let rounded = count.round();
            if (count - rounded).abs() < f64::EPSILON
                && rounded >= 0.0
                && rounded <= max_underscores
            {
                format!("{rounded:.0}")
            } else {
                String::new()
            }
        })
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    for (index, group) in grouped.groups.iter().enumerate() {
        draw_group(&mut chart, group, index == 0)?;
    }

    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}

/// Draws one box with its whiskers, capping the median line in red. The first
/// group also labels each part so the legend explains the glyphs.
fn draw_group(
    chart: &mut ChartContext<'_, BitMapBackend<'_>, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    group: &EloGroup,
    labelled: bool,
) -> Result<(), Box<dyn Error>> {
    let center = group.underscores as f64;
    let left = center - BOX_HALF_WIDTH;
    let right = center + BOX_HALF_WIDTH;
    let whisker = BASE_800.stroke_width(LINE_WIDTH);

    for (from, to) in [
        (group.low, group.lower_quartile),
        (group.upper_quartile, group.high),
    ] {
        chart.draw_series(LineSeries::new([(center, from), (center, to)], whisker))?;
    }
    for cap in [group.low, group.high] {
        chart.draw_series(LineSeries::new(
            [
                (center - CAP_HALF_WIDTH, cap),
                (center + CAP_HALF_WIDTH, cap),
            ],
            whisker,
        ))?;
    }

    let extent = PathElement::new(vec![(center, group.low), (center, group.high)], whisker);
    let extent = chart.draw_series(std::iter::once(extent.into_dyn()))?;
    if labelled {
        extent
            .label("Whiskers (furthest value within 1.5xIQR)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BASE_800));
    }

    let box_fill = Rectangle::new(
        [(left, group.lower_quartile), (right, group.upper_quartile)],
        BLUE_600.mix(BOX_OPACITY).filled(),
    );
    let interquartile = chart.draw_series(std::iter::once(box_fill.into_dyn()))?;
    if labelled {
        interquartile.label("Interquartile range").legend(|(x, y)| {
            Rectangle::new(
                [(x, y - 6), (x + LEGEND_WIDTH, y + 6)],
                BLUE_600.mix(BOX_OPACITY).filled(),
            )
        });
    }

    let box_border = Rectangle::new(
        [(left, group.lower_quartile), (right, group.upper_quartile)],
        BASE_800.stroke_width(LINE_WIDTH),
    );
    chart.draw_series(std::iter::once(box_border.into_dyn()))?;

    let median_line = PathElement::new(
        vec![(left, group.median), (right, group.median)],
        RED_600.stroke_width(MEDIAN_WIDTH),
    );
    let median = chart.draw_series(std::iter::once(median_line.into_dyn()))?;
    if labelled {
        median.label("Median").legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + LEGEND_WIDTH, y)],
                RED_600.stroke_width(MEDIAN_WIDTH),
            )
        });
    }

    Ok(())
}
