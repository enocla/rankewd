use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, HOURS_PER_DAY, load_duels, percentile};
use crate::plot::{
    BLUE_600, Line, LineChart, RED_600, YELLOW_600, draw_line_chart, integer_tick, season_span,
    value_range,
};
use chrono::{DateTime, Timelike, Utc};
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/matchmaking_gap_by_hour.png";

pub(super) struct MatchmakingGapByHour;

impl Analysis for MatchmakingGapByHour {
    fn name(&self) -> &'static str {
        "matchmaking-gap-by-hour"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut gaps: Vec<Vec<f64>> = vec![Vec::new(); HOURS_PER_DAY];
        for duel in &duels.duels {
            let (Some((first, second)), Some(time)) =
                (duel.elos(), DateTime::<Utc>::from_timestamp(duel.date, 0))
            else {
                continue;
            };
            gaps[time.hour() as usize].push(f64::from((first - second).abs()));
        }
        if gaps.iter().any(Vec::is_empty) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "some UTC hour has no rated match",
            )
            .into());
        }
        for hour in &mut gaps {
            hour.sort_unstable_by(f64::total_cmp);
        }
        let series = |fraction: f64| -> Vec<(f64, f64)> {
            gaps.iter()
                .enumerate()
                .map(|(hour, values)| (hour as f64, percentile(values, fraction)))
                .collect()
        };
        let (median, upper, tail) = (series(0.5), series(0.75), series(0.9));
        let y_range = value_range(tail.iter().chain(&median).map(|p| p.1), 0.1, 0.35);

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_line_chart(
            &output,
            &LineChart {
                title: format!(
                    "{}: Loosest Matchmaking at {:02.0}:00 UTC",
                    season_span(&seasons),
                    tail.iter()
                        .copied()
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                        .unwrap_or_default()
                        .0
                ),
                x_desc: "Hour of day (UTC)",
                y_desc: "Pre-match Elo gap",
                x_range: -0.5..HOURS_PER_DAY as f64 - 0.5,
                y_range: 0.0..y_range.end,
                x_labels: 9,
                x_format: &|hour| format!("{:02.0}:00", hour.round()),
                y_format: &integer_tick,
                lines: vec![
                    Line::new("90th percentile", RED_600, tail.clone()),
                    Line::new("75th percentile", YELLOW_600, upper),
                    Line::new("Median", BLUE_600, median.clone()),
                ],
                reference: None,
            },
        )?;

        let (widest, _) = median
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .unwrap_or((0, &(0.0, 0.0)));
        let (tightest, _) = median
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
            .unwrap_or((0, &(0.0, 0.0)));
        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: {} rated matches by UTC hour",
            season_span(&seasons),
            gaps.iter().map(Vec::len).sum::<usize>()
        ));
        report.line(format!(
            "  Widest median gap {:02}:00 UTC ({:.0}, p90 {:.0}); tightest {:02}:00 UTC ({:.0}, p90 {:.0})",
            widest,
            median[widest].1,
            tail[widest].1,
            tightest,
            median[tightest].1,
            tail[tightest].1
        ));
        report.line(format!("  Matchmaking gap by hour: {}", output.display()));
        Ok(report)
    }
}
