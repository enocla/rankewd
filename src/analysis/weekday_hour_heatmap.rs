use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, HOURS_PER_DAY, load_duels};
use crate::plot::{
    BASE_800, CELL_FONT_SIZE, FONT_FAMILY, canvas, chart_builder, diverging_color, season_span,
    style_mesh, text_on,
};
use chrono::{DateTime, Datelike, Timelike, Utc};
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/weekday_hour_heatmap.png";
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub(super) struct WeekdayHourHeatmap;

impl Analysis for WeekdayHourHeatmap {
    fn name(&self) -> &'static str {
        "weekday-hour-heatmap"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut counts = [[0u64; HOURS_PER_DAY]; 7];
        for duel in &duels.duels {
            let Some(time) = DateTime::<Utc>::from_timestamp(duel.date, 0) else {
                continue;
            };
            counts[time.weekday().num_days_from_monday() as usize][time.hour() as usize] += 1;
        }
        let total: u64 = counts.iter().flatten().sum();
        if total == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no matches").into());
        }
        let mean = total as f64 / (7 * HOURS_PER_DAY) as f64;
        let index: Vec<Vec<f64>> = counts
            .iter()
            .map(|row| row.iter().map(|count| *count as f64 / mean).collect())
            .collect();
        let (busiest, quietest) = extremes(&index);

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_chart(&output, &season_span(&seasons), &index, busiest)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: {total} matches by UTC weekday and hour; 1.0 is the weekly mean hour",
            season_span(&seasons)
        ));
        report.line(format!(
            "  Busiest {} {:02}:00 UTC at {:.2}x the mean; quietest {} {:02}:00 UTC at {:.2}x",
            DAYS[busiest.0],
            busiest.1,
            index[busiest.0][busiest.1],
            DAYS[quietest.0],
            quietest.1,
            index[quietest.0][quietest.1]
        ));
        let day_totals: Vec<String> = counts
            .iter()
            .zip(DAYS)
            .map(|(row, day)| {
                format!(
                    "{day} {:.1}%",
                    100.0 * row.iter().sum::<u64>() as f64 / total as f64
                )
            })
            .collect();
        report.line(format!("  Share by day: {}", day_totals.join(", ")));
        report.line(format!("  Weekday hour heatmap: {}", output.display()));
        Ok(report)
    }
}

fn extremes(index: &[Vec<f64>]) -> ((usize, usize), (usize, usize)) {
    let mut busiest = (0, 0);
    let mut quietest = (0, 0);
    for (day, row) in index.iter().enumerate() {
        for (hour, value) in row.iter().enumerate() {
            if *value > index[busiest.0][busiest.1] {
                busiest = (day, hour);
            }
            if *value < index[quietest.0][quietest.1] {
                quietest = (day, hour);
            }
        }
    }
    (busiest, quietest)
}

fn draw_chart(
    path: &Path,
    span: &str,
    index: &[Vec<f64>],
    busiest: (usize, usize),
) -> Result<(), Box<dyn Error>> {
    let root = canvas(path)?;
    let hours = HOURS_PER_DAY as f64;
    let mut chart = chart_builder(
        &root,
        &format!(
            "{span}: Weekly Rhythm (peak {} {:02}:00 UTC)",
            DAYS[busiest.0], busiest.1
        ),
    )
    .build_cartesian_2d(-0.5f64..hours - 0.5, 6.5f64..-0.5f64)?;
    style_mesh(&mut chart.configure_mesh(), "Hour of day (UTC)", "Weekday")
        .disable_mesh()
        .x_labels(HOURS_PER_DAY * 2 + 1)
        .y_labels(15)
        .x_label_formatter(&|value| {
            let hour = value.round();
            if (value - hour).abs() < 1e-6 && (hour as i64) % 3 == 0 {
                format!("{hour:02.0}")
            } else {
                String::new()
            }
        })
        .y_label_formatter(&|value| {
            let day = value.round();
            if (value - day).abs() < 1e-6 && (0.0..7.0).contains(&day) {
                DAYS[day as usize].to_owned()
            } else {
                String::new()
            }
        })
        .draw()?;

    let centered = Pos::new(HPos::Center, VPos::Center);
    for (day, row) in index.iter().enumerate() {
        for (hour, value) in row.iter().enumerate() {
            let (x, y) = (hour as f64, day as f64);
            // Log scale centered on the mean hour: equally busier and quieter hours
            // read as equally strong blue and red.
            let fill = diverging_color(value.max(1e-3).log2(), 0.8);
            chart.draw_series(std::iter::once(Rectangle::new(
                [(x - 0.5, y - 0.5), (x + 0.5, y + 0.5)],
                fill.filled(),
            )))?;
            chart.draw_series(std::iter::once(Text::new(
                format!("{value:.1}"),
                (x, y),
                (FONT_FAMILY, CELL_FONT_SIZE)
                    .into_font()
                    .color(&text_on(fill))
                    .pos(centered),
            )))?;
        }
    }
    let (day, hour) = (busiest.0 as f64, busiest.1 as f64);
    chart.draw_series(std::iter::once(Rectangle::new(
        [(hour - 0.5, day - 0.5), (hour + 0.5, day + 0.5)],
        BASE_800.stroke_width(3),
    )))?;
    root.present()?;
    Ok(())
}
