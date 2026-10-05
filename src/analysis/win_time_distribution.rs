use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels, median};
use crate::plot::{
    Line, LineChart, draw_line_chart, format_clock, progress_color, season_span, value_range,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/win_time_distribution.png";
/// Histogram bin width, in seconds.
const BIN_SECONDS: u32 = 30;
/// Charted winning-time window, in minutes.
const FIRST_MINUTE: u32 = 5;
const LAST_MINUTE: u32 = 35;
/// Winning time the report measures each season against, in minutes.
const FAST_MINUTES: u32 = 10;
const MIN_SEASON_WINS: usize = 500;

pub(super) struct WinTimeDistribution;

impl Analysis for WinTimeDistribution {
    fn name(&self) -> &'static str {
        "win-time-distribution"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut by_season: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
        for duel in &duels.duels {
            if let (Some(_), Some(time)) = (duel.winner, duel.duration_ms)
                && time > 0
            {
                by_season
                    .entry(duel.season)
                    .or_default()
                    .push(f64::from(time) / 1000.0);
            }
        }
        by_season.retain(|_, times| times.len() >= MIN_SEASON_WINS);
        if by_season.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no winning times").into());
        }

        let first = f64::from(FIRST_MINUTE * 60);
        let bins = usize::try_from((LAST_MINUTE - FIRST_MINUTE) * 60 / BIN_SECONDS)?;
        let season_count = by_season.len();
        let mut lines = Vec::new();
        let mut report = AnalysisReport::default();
        let mut medians: Vec<(u32, f64)> = Vec::new();
        for (position, (season, times)) in by_season.iter_mut().enumerate() {
            let mut counts = vec![0u64; bins];
            for time in times.iter() {
                let offset = (time - first) / f64::from(BIN_SECONDS);
                if offset >= 0.0 && (offset as usize) < bins {
                    counts[offset as usize] += 1;
                }
            }
            let total = times.len() as f64;
            let points = counts
                .iter()
                .enumerate()
                .map(|(index, count)| {
                    let minute = (first + (index as f64 + 0.5) * f64::from(BIN_SECONDS)) / 60.0;
                    (minute, *count as f64 / total)
                })
                .collect();
            let fast = times
                .iter()
                .filter(|time| **time < f64::from(FAST_MINUTES * 60))
                .count();
            let median_seconds = median(times).unwrap_or_default();
            medians.push((*season, median_seconds));
            report.line(format!(
                "Season {season}: {} wins, median {:.0}:{:02.0}, {:.1}% under {FAST_MINUTES} minutes",
                times.len(),
                (median_seconds / 60.0).floor(),
                median_seconds % 60.0,
                100.0 * fast as f64 / total
            ));
            let progress = position as f64 / (season_count.max(2) - 1) as f64;
            let mut line = Line::new(format!("S{season}"), progress_color(progress), points);
            line.markers = false;
            lines.push(line);
        }

        let seasons: Vec<u32> = by_season.keys().copied().collect();
        let fastest = medians
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or_default();
        let slowest = medians
            .iter()
            .copied()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or_default();
        let y_range = value_range(
            lines
                .iter()
                .flat_map(|line| line.points.iter().map(|p| p.1)),
            0.0,
            0.08,
        );
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!(
                    "{}: Median Win {} in S{} vs {} in S{}",
                    season_span(&seasons),
                    format_clock(fastest.1 * 1000.0),
                    fastest.0,
                    format_clock(slowest.1 * 1000.0),
                    slowest.0
                ),
                x_desc: "Winning time (minutes)",
                y_desc: "Share of wins per 30 s",
                x_range: f64::from(FIRST_MINUTE)..f64::from(LAST_MINUTE),
                y_range: 0.0..y_range.end,
                x_labels: 7,
                x_format: &|minute| format!("{minute:.0}"),
                y_format: &|share| format!("{:.1}%", share * 100.0),
                lines,
                reference: None,
            },
        )?;
        report.line(format!("  Win time distribution: {}", output.display()));
        Ok(report)
    }
}
