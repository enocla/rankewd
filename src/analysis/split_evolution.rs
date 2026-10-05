use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, MILESTONES, load_duels, median};
use crate::plot::{
    BASE_800, POINT_RADIUS, SERIES_COLORS, SERIES_STROKE, canvas, chart_builder,
    draw_series_legend, format_clock, legend_line, season_span, season_tick, style_mesh,
};
use plotters::prelude::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/split_evolution.png";
/// Winning runs a season needs before its medians are charted.
const MIN_SEASON_WINS: usize = 500;

pub(super) struct SplitEvolution;

/// One chart line: its label, color, and (season position, minutes) points.
type Series = (String, RGBColor, Vec<(f64, f64)>);

/// One season's median winner split per milestone, plus the median finish.
struct SeasonSplits {
    season: u32,
    wins: usize,
    milestones: Vec<Option<f64>>,
    finish: f64,
}

impl Analysis for SplitEvolution {
    fn name(&self) -> &'static str {
        "split-evolution"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns {
                timelines: true,
                ..DuelColumns::default()
            },
        )?;

        // Per season: the winner's split at every milestone and the finish time.
        let mut by_season: BTreeMap<u32, (Vec<Vec<f64>>, Vec<f64>)> = BTreeMap::new();
        for duel in &duels.duels {
            let (Some(winner), Some(finish), true) =
                (duel.winner, duel.duration_ms, duel.has_timeline)
            else {
                continue;
            };
            let splits = &duel.sides[winner].splits;
            // Only runs that record every milestone describe a full route.
            if splits.iter().any(Option::is_none) {
                continue;
            }
            let (milestones, finishes) = by_season
                .entry(duel.season)
                .or_insert_with(|| (vec![Vec::new(); MILESTONES.len()], Vec::new()));
            for (index, split) in splits.iter().enumerate() {
                if let Some(split) = split {
                    milestones[index].push(f64::from(split.get()));
                }
            }
            finishes.push(f64::from(finish));
        }

        let mut seasons = Vec::new();
        for (season, (mut milestones, mut finishes)) in by_season {
            if finishes.len() < MIN_SEASON_WINS {
                continue;
            }
            seasons.push(SeasonSplits {
                season,
                wins: finishes.len(),
                milestones: milestones.iter_mut().map(|values| median(values)).collect(),
                finish: median(&mut finishes).unwrap_or_default(),
            });
        }
        if seasons.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no season had enough full-route wins",
            )
            .into());
        }

        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        let labels: Vec<u32> = seasons.iter().map(|season| season.season).collect();
        draw_chart(&output, &season_span(&labels), &seasons)?;

        let mut report = AnalysisReport::default();
        for season in &seasons {
            let splits = season
                .milestones
                .iter()
                .zip(MILESTONES)
                .map(|(split, milestone)| {
                    format!(
                        "{} {}",
                        milestone.label(),
                        split.map_or_else(|| "-".to_owned(), format_clock)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            report.line(format!(
                "Season {} ({} full-route wins): {splits}, finish {}",
                season.season,
                season.wins,
                format_clock(season.finish)
            ));
        }
        report.line(format!("  Split evolution: {}", output.display()));
        Ok(report)
    }
}

fn draw_chart(path: &Path, span: &str, seasons: &[SeasonSplits]) -> Result<(), Box<dyn Error>> {
    let labels: Vec<u32> = seasons.iter().map(|season| season.season).collect();
    let max_minutes = seasons
        .iter()
        .map(|season| season.finish / 60_000.0)
        .fold(0.0, f64::max)
        * 1.4;
    let last = (seasons.len() - 1) as f64;

    let root = canvas(path)?;
    let slowest = seasons
        .iter()
        .max_by(|a, b| a.finish.total_cmp(&b.finish))
        .map_or((0, 0.0), |season| (season.season, season.finish));
    let title = format!(
        "{span}: Median Winning Splits (slowest S{} at {})",
        slowest.0,
        format_clock(slowest.1)
    );
    let mut chart =
        chart_builder(&root, &title).build_cartesian_2d(-0.3f64..last + 0.3, 0f64..max_minutes)?;
    style_mesh(&mut chart.configure_mesh(), "Season", "Minutes into match")
        .x_labels(seasons.len() * 2 + 1)
        .x_label_formatter(&|position| season_tick(&labels, *position))
        .y_label_formatter(&|minutes| format!("{minutes:.0}"))
        .draw()?;

    let mut series: Vec<Series> = MILESTONES
        .iter()
        .enumerate()
        .map(|(index, milestone)| {
            let points = seasons
                .iter()
                .enumerate()
                .filter_map(|(position, season)| {
                    season.milestones[index].map(|split| (position as f64, split / 60_000.0))
                })
                .collect();
            (
                milestone.label().to_owned(),
                SERIES_COLORS[index % SERIES_COLORS.len()],
                points,
            )
        })
        .collect();
    series.push((
        "Finish".to_owned(),
        BASE_800,
        seasons
            .iter()
            .enumerate()
            .map(|(position, season)| (position as f64, season.finish / 60_000.0))
            .collect(),
    ));

    // Draw the latest milestone first so the legend reads top-down like the lines.
    for (label, color, points) in series.iter().rev() {
        chart
            .draw_series(LineSeries::new(
                points.iter().copied(),
                color.stroke_width(SERIES_STROKE),
            ))?
            .label(label.as_str())
            .legend(legend_line(*color));
        chart.draw_series(
            points
                .iter()
                .map(|point| Circle::new(*point, POINT_RADIUS + 2, color.filled())),
        )?;
    }
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
