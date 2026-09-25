use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{PlayerSeasons, SeasonData, elo_range};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER, POINT_RADIUS,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend, format_correlation,
    log_key_points, padded_bounds, pearson_r,
};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "elo_vs_win_duration_season_";
const MATCH_TYPE: i64 = 2;
const MILLISECONDS_PER_SECOND: f64 = 1_000.0;
/// Wins required before a player's mean duration is stable enough to correlate.
const MIN_WINS: usize = 10;
const X_AXIS_TITLE: &str = "Final Elo in season";
const Y_AXIS_TITLE: &str = "Average win duration (seconds)";

pub(super) struct EloVersusWinDuration;

#[derive(Default)]
struct PlayerMeans {
    points: Vec<(i64, f64)>,
    rated_players: usize,
    too_few_wins: usize,
}

impl Analysis for EloVersusWinDuration {
    fn name(&self) -> &'static str {
        "elo-versus-win-duration"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;

        let mut report = AnalysisReport::default();
        for season in data.all_player_seasons() {
            let means = average_win_durations(&season);
            if means.points.is_empty() {
                report.line(format!(
                    "Season {}: no rated player had at least {MIN_WINS} qualifying timed wins",
                    season.season
                ));
                continue;
            }

            let correlation = pearson_r(&means.points);
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &means.points, correlation)?;
            report.line(format!(
                "Season {}: {} of {} rated players had at least {MIN_WINS} type-{MATCH_TYPE} wins; {} had fewer",
                season.season,
                means.points.len(),
                means.rated_players,
                means.too_few_wins
            ));
            report.line(format!(
                "  R (Elo vs average win duration): {}",
                format_correlation(correlation)
            ));
            report.line(format!("  Elo vs average win duration: {}", path.display()));
        }
        Ok(report)
    }
}

/// Mean duration of each rated player's qualifying wins, in seconds. Wins are
/// type-2, non-decayed, non-forfeited matches the player won, timed by
/// `result.time`.
fn average_win_durations(season: &SeasonData<'_>) -> PlayerMeans {
    let mut means = PlayerMeans::default();

    for player in season
        .players
        .iter()
        .filter(|player| player.latest_elo.is_some())
    {
        means.rated_players += 1;
        let Some(elo) = player.latest_elo else {
            continue;
        };
        let Some(events) = season.history.player_events(&player.uuid) else {
            means.too_few_wins += 1;
            continue;
        };

        let mut total_ms = 0u128;
        let mut wins = 0usize;
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
            total_ms += u128::from(result_time_ms);
            wins += 1;
        }

        if wins < MIN_WINS {
            means.too_few_wins += 1;
            continue;
        }
        means
            .points
            .push((elo, total_ms as f64 / wins as f64 / MILLISECONDS_PER_SECOND));
    }

    means
}

fn draw_chart(
    path: &Path,
    season: u32,
    points: &[(i64, f64)],
    correlation: Option<f64>,
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let (min_elo, max_elo) = elo_range(points.iter().map(|(elo, _)| *elo))?;
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);
    let (y_min, y_max) = duration_bounds(points)?;
    let y_key_points = log_key_points(y_min, y_max);
    let caption_r = match correlation {
        Some(value) => format!("{value:.3}"),
        None => "n/a".to_owned(),
    };

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Elo vs Average Win Duration (r = {caption_r})"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(
            x_min..x_max,
            (y_min..y_max).log_scale().with_key_points(y_key_points),
        )?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(8)
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

    chart
        .draw_series(points.iter().map(|(elo, seconds)| {
            Circle::new((*elo, *seconds), POINT_RADIUS, BLUE_600.mix(0.25).filled())
        }))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}

/// Log-scaled bounds that keep the shortest and longest means clear of the frame.
fn duration_bounds(points: &[(i64, f64)]) -> Result<(f64, f64), Box<dyn Error>> {
    let min = points
        .iter()
        .map(|(_, seconds)| *seconds)
        .fold(f64::INFINITY, f64::min);
    let max = points
        .iter()
        .map(|(_, seconds)| *seconds)
        .fold(f64::NEG_INFINITY, f64::max);
    if !min.is_finite() || !max.is_finite() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no average win durations to plot",
        )
        .into());
    }
    Ok((min, max * 1.1))
}
