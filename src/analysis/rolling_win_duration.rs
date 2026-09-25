use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{PlayerSeasons, SeasonData};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, GREEN_600, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend, log_key_points,
};
use plotters::prelude::*;
use std::collections::VecDeque;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "rolling_win_duration_season_";
const MIN_GAMES: u64 = 100;
const MATCH_TYPE: i64 = 2;
const ROLLING_WINS: usize = 10;
const MILLISECONDS_PER_SECOND: u64 = 1_000;
const TREND_BINS: usize = 50;
const TREND_COMPARISON_POINTS: usize = 10;
/// Season progress before this fraction is ignored when comparing the opening
/// and closing durations reported as the season's improvement.
const COMPARISON_SKIP_PROGRESS: f64 = 0.10;
const MIN_TREND_POINTS: usize = 20;
const X_AXIS_TITLE: &str = "Season progress";
const Y_AXIS_TITLE: &str = "Seconds (log scale)";

pub(super) struct RollingWinDuration;

#[derive(Clone, Copy)]
struct DurationPoint {
    progress: f64,
    seconds: f64,
}

struct PlayerSeries {
    points: Vec<DurationPoint>,
}

struct SeriesCollection {
    eligible_players: usize,
    omitted_players: usize,
    players: Vec<PlayerSeries>,
    trend: Vec<DurationPoint>,
}

impl Analysis for RollingWinDuration {
    fn name(&self) -> &'static str {
        "rolling-win-duration"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;

        let mut report = AnalysisReport::default();
        for season in data.all_player_seasons() {
            let series = collect_series(&season)?;
            if series.eligible_players == 0 {
                report.line(format!(
                    "Season {}: no players with more than {MIN_GAMES} games",
                    season.season
                ));
                continue;
            }
            if series.players.is_empty() {
                report.line(format!(
                    "Season {}: {} players qualified, but none had {ROLLING_WINS} qualifying timed wins",
                    season.season, series.eligible_players
                ));
                continue;
            }

            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &series.players, &series.trend)?;
            report.line(format!(
                "Season {}: {} of {} players with more than {MIN_GAMES} games plotted from type-{MATCH_TYPE}, non-decayed, non-forfeited wins; {} lacked {ROLLING_WINS} qualifying timed wins",
                season.season,
                series.players.len(),
                series.eligible_players,
                series.omitted_players
            ));
            if let Some(improvement) = improvement(&series.trend) {
                report.line(improvement.report());
            }
            report.line(format!("  Rolling win duration: {}", path.display()));
        }
        Ok(report)
    }
}

fn collect_series(season: &SeasonData<'_>) -> Result<SeriesCollection, Box<dyn Error>> {
    let (first_date, _, duration) = season.history.timeline()?;
    let mut collection = SeriesCollection {
        eligible_players: 0,
        omitted_players: 0,
        players: Vec::new(),
        trend: Vec::new(),
    };

    for player in season
        .players
        .iter()
        .copied()
        .filter(|player| player.games_played > MIN_GAMES)
    {
        collection.eligible_players += 1;
        let events = season
            .history
            .player_events(&player.uuid)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "missing eligible player history",
                )
            })?
            .to_vec();
        let mut events = events;
        events.sort_unstable_by_key(|event| (event.date, event.match_id));

        let mut recent_wins = VecDeque::with_capacity(ROLLING_WINS);
        let mut duration_sum = 0u128;
        let mut points = Vec::new();
        for event in events {
            if !event.won || event.match_type != Some(MATCH_TYPE) {
                continue;
            }
            let Some(result_time_ms) = event
                .result_time_ms
                .and_then(|time| u64::try_from(time).ok())
                .filter(|time| *time > 0)
            else {
                continue;
            };
            if recent_wins.len() == ROLLING_WINS {
                duration_sum -= u128::from(recent_wins.pop_front().expect("full rolling window"));
            }
            recent_wins.push_back(result_time_ms);
            duration_sum += u128::from(result_time_ms);
            if recent_wins.len() == ROLLING_WINS {
                points.push(DurationPoint {
                    progress: progress_of(event.date, first_date, duration),
                    seconds: duration_sum as f64
                        / ROLLING_WINS as f64
                        / MILLISECONDS_PER_SECOND as f64,
                });
            }
        }

        if points.is_empty() {
            collection.omitted_players += 1;
        } else {
            collection.players.push(PlayerSeries { points });
        }
    }

    collection.trend = mean_trend(&collection.players);
    Ok(collection)
}

fn progress_of(date: i64, first_date: i64, duration: f64) -> f64 {
    ((date.saturating_sub(first_date) as f64) / duration).clamp(0.0, 1.0)
}

fn mean_trend(players: &[PlayerSeries]) -> Vec<DurationPoint> {
    let player_bins: Vec<_> = players
        .iter()
        .map(|player| {
            let mut bins = vec![Vec::new(); TREND_BINS];
            for point in &player.points {
                let bin = (point.progress * TREND_BINS as f64) as usize;
                bins[bin.min(TREND_BINS - 1)].push(point.seconds);
            }
            bins
        })
        .collect();
    let mut cohort_bins = vec![Vec::new(); TREND_BINS];
    for bins in player_bins {
        for (bin, values) in bins.into_iter().enumerate() {
            if !values.is_empty() {
                cohort_bins[bin].push(mean(&values));
            }
        }
    }

    cohort_bins
        .into_iter()
        .enumerate()
        .filter(|(_, values)| values.len() >= MIN_TREND_POINTS)
        .map(|(bin, values)| DurationPoint {
            progress: (bin as f64 + 0.5) / TREND_BINS as f64,
            seconds: mean(&values),
        })
        .collect()
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn mean_seconds(points: &[DurationPoint]) -> f64 {
    points.iter().map(|point| point.seconds).sum::<f64>() / points.len() as f64
}

/// Season-over-season gain from the opening and closing mean durations, both
/// taken from the part of the season that starts at [`COMPARISON_SKIP_PROGRESS`].
struct Improvement {
    early_seconds: f64,
    late_seconds: f64,
    early_span: (f64, f64),
    late_span: (f64, f64),
}

impl Improvement {
    fn change(&self) -> f64 {
        (self.late_seconds - self.early_seconds) / self.early_seconds * 100.0
    }

    fn report(&self) -> String {
        let change = self.change();
        let direction = if change < 0.0 {
            "improvement"
        } else if change > 0.0 {
            "slowdown"
        } else {
            "no change"
        };
        format!(
            "  Mean player duration, {:.0}-{:.0}% to {:.0}-{:.0}%: {:.1}s -> {:.1}s ({:.1}% {direction})",
            self.early_span.0 * 100.0,
            self.early_span.1 * 100.0,
            self.late_span.0 * 100.0,
            self.late_span.1 * 100.0,
            self.early_seconds,
            self.late_seconds,
            change.abs()
        )
    }
}

fn improvement(trend: &[DurationPoint]) -> Option<Improvement> {
    let start = trend.partition_point(|point| point.progress < COMPARISON_SKIP_PROGRESS);
    let retained = &trend[start..];
    let comparison_points = TREND_COMPARISON_POINTS.min(retained.len() / 2);
    if comparison_points == 0 {
        return None;
    }
    let early = &retained[..comparison_points];
    let late = &retained[retained.len() - comparison_points..];
    Some(Improvement {
        early_seconds: mean_seconds(early),
        late_seconds: mean_seconds(late),
        early_span: (early[0].progress, early[comparison_points - 1].progress),
        late_span: (late[0].progress, late[comparison_points - 1].progress),
    })
}

fn duration_bounds(players: &[PlayerSeries]) -> Result<(f64, f64), Box<dyn Error>> {
    let mut bounds: Option<(f64, f64)> = None;
    for point in players.iter().flat_map(|player| &player.points) {
        bounds = Some(match bounds {
            Some((min, max)) => (min.min(point.seconds), max.max(point.seconds)),
            None => (point.seconds, point.seconds),
        });
    }
    bounds.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "no rolling durations to plot").into()
    })
}

fn draw_chart(
    path: &Path,
    season: u32,
    players: &[PlayerSeries],
    trend: &[DurationPoint],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let (min_seconds, max_seconds) = duration_bounds(players)?;
    let y_min = min_seconds;
    let y_max = (max_seconds * 1.1).max(y_min * 10.0);
    let y_key_points = log_key_points(y_min, y_max);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: 10-Win Rolling Duration"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(
            0.0f64..1.0,
            (y_min..y_max).log_scale().with_key_points(y_key_points),
        )?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .x_label_formatter(&|progress| format!("{:.0}%", progress * 100.0))
        .y_labels(8)
        .y_label_formatter(&|seconds| {
            if *seconds >= 1_000.0 {
                format!("{:.0}k", seconds / 1_000.0)
            } else if *seconds >= 1.0 {
                format!("{seconds:.0}")
            } else {
                format!("{seconds:.3}")
            }
        })
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    let style = BLUE_600.mix(0.04).stroke_width(1);
    let first = players
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no player lines to plot"))?;
    chart
        .draw_series(LineSeries::new(
            first
                .points
                .iter()
                .map(|point| (point.progress, point.seconds)),
            style,
        ))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    for player in &players[1..] {
        chart.draw_series(LineSeries::new(
            player
                .points
                .iter()
                .map(|point| (point.progress, point.seconds)),
            style,
        ))?;
    }
    if trend.len() > 1 {
        chart
            .draw_series(LineSeries::new(
                trend.iter().map(|point| (point.progress, point.seconds)),
                GREEN_600.stroke_width(4),
            ))?
            .label("Mean player (lower is better)")
            .legend(|(x, y)| {
                PathElement::new(
                    vec![(x, y), (x + LEGEND_WIDTH, y)],
                    GREEN_600.stroke_width(4),
                )
            });
    }
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
