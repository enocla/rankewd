use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{HOURS_PER_DAY, SeasonHours, load_season_hours};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, YELLOW_600, create_parent, draw_series_legend,
};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "games_per_hour_season_";
const X_AXIS_TITLE: &str = "Hour of day (UTC)";
const Y_AXIS_TITLE: &str = "Matches";
/// Ticks requested on the hour axis; six labelled hours fit across the day.
const HOUR_LABEL_COUNT: usize = 6;
const BAR_OPACITY: f64 = 0.85;

pub(super) struct GamesPerHour;

/// One season's hour-of-day match counts, with the hours that stand out.
struct HourlyMatches {
    season: u32,
    matches: [u64; HOURS_PER_DAY],
    total: u64,
    forfeited: u64,
    busiest: usize,
    quietest: usize,
}

impl Analysis for GamesPerHour {
    fn name(&self) -> &'static str {
        "games-per-hour"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let seasons = load_season_hours(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;

        let mut report = AnalysisReport::default();
        for season in &seasons {
            let hourly = summarize(season);
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", hourly.season));
            draw_chart(&path, &hourly)?;
            report.line(format!(
                "Season {}: {} non-decayed matches by UTC hour, including {} forfeited ({:.2}%)",
                hourly.season,
                hourly.total,
                hourly.forfeited,
                percent_of(hourly.forfeited, hourly.total)
            ));
            report.line(format!(
                "  Busiest hour {:02}:00 UTC: {} matches ({:.2}%); quietest {:02}:00 UTC: {} matches ({:.2}%)",
                hourly.busiest,
                hourly.matches[hourly.busiest],
                percent_of(hourly.matches[hourly.busiest], hourly.total),
                hourly.quietest,
                hourly.matches[hourly.quietest],
                percent_of(hourly.matches[hourly.quietest], hourly.total)
            ));
            report.line(format!("  Matches by UTC hour: {}", path.display()));
        }
        Ok(report)
    }
}

/// Hour of day with the most matches and the hour with the fewest. Ties keep
/// the earlier hour.
fn extremes(matches: &[u64; HOURS_PER_DAY]) -> (usize, usize) {
    let mut busiest = (0, matches[0]);
    let mut quietest = (0, matches[0]);
    for (hour, count) in matches.iter().enumerate().skip(1) {
        if *count > busiest.1 {
            busiest = (hour, *count);
        }
        if *count < quietest.1 {
            quietest = (hour, *count);
        }
    }
    (busiest.0, quietest.0)
}

fn summarize(season: &SeasonHours) -> HourlyMatches {
    let (busiest, quietest) = extremes(&season.matches);
    HourlyMatches {
        season: season.season,
        matches: season.matches,
        total: season.match_count(),
        forfeited: season.forfeited_matches,
        busiest,
        quietest,
    }
}

/// Share of `total` taken by `part`, in percent.
fn percent_of(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    part as f64 / total as f64 * 100.0
}

fn draw_chart(path: &Path, data: &HourlyMatches) -> Result<(), Box<dyn Error>> {
    if data.total == 0 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no matches to plot by hour").into(),
        );
    }
    create_parent(path)?;

    let max_matches = data.matches.iter().copied().max().unwrap_or(0).max(1);
    let day_end = i64::try_from(HOURS_PER_DAY)?;
    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    // Each bar spans the full width of its hour, so the day reads as 24
    // equal bins from midnight to midnight.
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!(
                "Season {}: Matches by UTC Hour (peak {:02}:00 UTC)",
                data.season, data.busiest
            ),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0..day_end, 0u64..(max_matches + max_matches / 10 + 1))?;

    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(HOUR_LABEL_COUNT)
        .y_labels(8)
        .x_label_formatter(&|hour| {
            if (0..day_end).contains(hour) {
                format!("{hour:02}:00")
            } else {
                String::new()
            }
        })
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    // Every bar is one (hour it starts in, matches) pair, so the busiest hour
    // can be pulled out of the same set the other hours are drawn from.
    let bars: Vec<_> = (0..day_end)
        .map(|hour| (hour, data.matches[usize::try_from(hour).unwrap_or(0)]))
        .collect();
    let busiest = i64::try_from(data.busiest).unwrap_or(day_end);
    let bar = |hour: i64, count: u64, color: RGBColor| {
        Rectangle::new(
            [(hour, 0), (hour + 1, count)],
            color.mix(BAR_OPACITY).filled(),
        )
    };
    chart
        .draw_series(
            bars.iter()
                .filter(|(hour, _)| *hour != busiest)
                .map(|(hour, count)| bar(*hour, *count, BLUE_600)),
        )?
        .label("Other hours")
        .legend(|(x, y)| {
            Rectangle::new(
                [(x, y - 5), (x + LEGEND_WIDTH, y + 5)],
                BLUE_600.mix(BAR_OPACITY).filled(),
            )
        });
    chart
        .draw_series(std::iter::once(bar(
            busiest,
            data.matches[data.busiest],
            YELLOW_600,
        )))?
        .label(format!("Busiest hour ({:02}:00 UTC)", data.busiest))
        .legend(|(x, y)| {
            Rectangle::new(
                [(x, y - 5), (x + LEGEND_WIDTH, y + 5)],
                YELLOW_600.mix(BAR_OPACITY).filled(),
            )
        });
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
