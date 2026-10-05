use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, HOURS_PER_DAY, load_duels, percentile};
use crate::plot::{
    BASE_800, BLUE_600, Line, LineChart, RED_600, YELLOW_600, draw_line_chart, integer_tick,
    season_span, value_range,
};
use chrono::{DateTime, Timelike, Utc};
use std::error::Error;
use std::io;

const DEFAULT_OUTPUT_FILE: &str = "output/elo_by_hour.png";

pub(super) struct EloByHour;

impl Analysis for EloByHour {
    fn name(&self) -> &'static str {
        "elo-by-hour"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut elos: Vec<Vec<f64>> = vec![Vec::new(); HOURS_PER_DAY];
        for duel in &duels.duels {
            let Some(time) = DateTime::<Utc>::from_timestamp(duel.date, 0) else {
                continue;
            };
            for side in &duel.sides {
                if let Some(elo) = side.elo_before {
                    elos[time.hour() as usize].push(f64::from(elo));
                }
            }
        }
        if elos.iter().any(Vec::is_empty) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "some UTC hour has no rated appearance",
            )
            .into());
        }
        for hour in &mut elos {
            hour.sort_unstable_by(f64::total_cmp);
        }
        let series = |fraction: f64| -> Vec<(f64, f64)> {
            elos.iter()
                .enumerate()
                .map(|(hour, values)| (hour as f64, percentile(values, fraction)))
                .collect()
        };
        let lines = vec![
            Line::new("99th percentile", BASE_800, series(0.99)),
            Line::new("90th percentile", RED_600, series(0.9)),
            Line::new("Median", BLUE_600, series(0.5)),
            Line::new("10th percentile", YELLOW_600, series(0.1)),
        ];
        let y_range = value_range(
            lines
                .iter()
                .flat_map(|line| line.points.iter().map(|p| p.1)),
            0.05,
            0.3,
        );
        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        let report_lines: Vec<String> = [0.5, 0.99]
            .iter()
            .map(|fraction| {
                let points = series(*fraction);
                let (low, high) = points.iter().fold(
                    (points[0], points[0]),
                    |(low, high), point| {
                        (
                            if point.1 < low.1 { *point } else { low },
                            if point.1 > high.1 { *point } else { high },
                        )
                    },
                );
                format!(
                    "  p{:.0} Elo peaks at {:02.0}:00 UTC ({:.0}) and bottoms at {:02.0}:00 UTC ({:.0})",
                    fraction * 100.0,
                    high.0,
                    high.1,
                    low.0,
                    low.1
                )
            })
            .collect();
        let top_peak = series(0.99)
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or_default()
            .0;
        draw_line_chart(
            &output,
            &LineChart {
                title: format!(
                    "{}: Queue Elo by Hour (top 1% peak {:02.0}:00 UTC)",
                    season_span(&seasons),
                    top_peak
                ),
                x_desc: "Hour of day (UTC)",
                y_desc: "Pre-match Elo of players",
                x_range: -0.5..HOURS_PER_DAY as f64 - 0.5,
                y_range,
                x_labels: 9,
                x_format: &|hour| format!("{:02.0}:00", hour.round()),
                y_format: &integer_tick,
                lines,
                reference: None,
            },
        )?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: {} rated appearances by UTC hour",
            season_span(&seasons),
            elos.iter().map(Vec::len).sum::<usize>()
        ));
        for line in report_lines {
            report.line(line);
        }
        report.line(format!("  Elo by hour: {}", output.display()));
        Ok(report)
    }
}
