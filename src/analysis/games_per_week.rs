use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset;
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_FONT_SIZE, LEGEND_WIDTH,
    PAPER, TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent,
};
use chrono::{DateTime, Datelike, Duration, Utc};
use parquet::file::reader::FileReader;
use parquet::schema::parser::parse_message_type;
use plotters::prelude::*;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::path::Path;

const DATE_SEASON_PROJECTION: &str = "message root { \
    optional int64 date; \
    optional int64 season; \
    optional boolean decayed; \
    optional boolean forfeited; \
}";
const DEFAULT_OUTPUT_FILE: &str = "output/games_per_week.png";
const CHART_TITLE: &str = "Games Played per Week";
const X_AXIS_TITLE: &str = "UTC week (ISO)";
const Y_AXIS_TITLE: &str = "Games played";
const MAX_X_LABELS: usize = 12;
const DATA_POINT_RADIUS: i32 = 4;

pub(super) struct GamesPerWeek;

type WeekCounts = BTreeMap<(i32, u32), u64>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WeeklyCount {
    pub label: String,
    pub games: u64,
}

impl Analysis for GamesPerWeek {
    fn name(&self) -> &'static str {
        "games-per-week"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let games_by_week = collect_games_by_week(input)?;
        let weekly_counts = complete_weeks(&games_by_week)?;
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_chart(&output, &weekly_counts)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "Wrote {} ({} weeks)",
            output.display(),
            weekly_counts.len()
        ));
        Ok(report)
    }
}

fn collect_games_by_week(
    input: &AnalysisInput<'_>,
) -> Result<BTreeMap<(i32, u32), u64>, Box<dyn Error>> {
    let mut games_by_week = BTreeMap::new();
    let projection = parse_message_type(DATE_SEASON_PROJECTION)?;

    for file in input.files() {
        let reader = dataset::open(&file.path)?;
        let chunks: Result<Vec<WeekCounts>, io::Error> = (0..reader.num_row_groups())
            .into_par_iter()
            .filter(|&index| dataset::row_group_matches(&reader, index, input.seasons()))
            .map(|index| {
                let worker_reader = dataset::open(&file.path)
                    .map_err(|error| io::Error::other(error.to_string()))?;
                let row_group = worker_reader
                    .get_row_group(index)
                    .map_err(io::Error::other)?;
                let mut counts = BTreeMap::new();
                for row in row_group
                    .get_row_iter(Some(projection.clone()))
                    .map_err(io::Error::other)?
                {
                    let row = row.map_err(io::Error::other)?;
                    if !dataset::passes_default_filters(&row)? {
                        continue;
                    }
                    let season = dataset::integer_field(&row, "season")?.ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "missing season")
                    })?;
                    let season = u32::try_from(season).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
                    })?;
                    if !input.includes_season(season) {
                        continue;
                    }

                    let timestamp = dataset::integer_field(&row, "date")?.ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "missing match date")
                    })?;
                    let date = DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "date is outside supported range",
                        )
                    })?;
                    let iso_week = date.iso_week();
                    *counts
                        .entry((iso_week.year(), iso_week.week()))
                        .or_default() += 1;
                }
                Ok(counts)
            })
            .collect();
        for counts in chunks? {
            for (week, games) in counts {
                *games_by_week.entry(week).or_default() += games;
            }
        }
    }

    if games_by_week.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no games found in selected files",
        )
        .into());
    }

    Ok(games_by_week)
}

fn complete_weeks(
    games_by_week: &BTreeMap<(i32, u32), u64>,
) -> Result<Vec<WeeklyCount>, Box<dyn Error>> {
    let first = games_by_week
        .keys()
        .next()
        .copied()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no games to plot"))?;
    let last = games_by_week
        .keys()
        .next_back()
        .copied()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no games to plot"))?;

    let mut week_start = iso_week_monday(first.0, first.1)?;
    let last_week_start = iso_week_monday(last.0, last.1)?;
    let mut weekly_counts = Vec::new();
    while week_start <= last_week_start {
        let iso_week = week_start.iso_week();
        weekly_counts.push(WeeklyCount {
            label: format!("{}-W{:02}", iso_week.year(), iso_week.week()),
            games: *games_by_week
                .get(&(iso_week.year(), iso_week.week()))
                .unwrap_or(&0),
        });
        week_start += Duration::weeks(1);
    }

    Ok(weekly_counts)
}

fn iso_week_monday(year: i32, week: u32) -> Result<chrono::NaiveDate, Box<dyn Error>> {
    chrono::NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid ISO week").into())
}

fn draw_chart(path: &Path, data: &[WeeklyCount]) -> Result<(), Box<dyn Error>> {
    if data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no weeks to plot").into());
    }
    create_parent(path)?;

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;

    let max_games = data.iter().map(|week| week.games).max().unwrap_or(0).max(1);
    let last_index = i32::try_from(data.len() - 1)?;
    let points = data
        .iter()
        .enumerate()
        .map(|(index, week)| Ok((i32::try_from(index)?, week.games)))
        .collect::<Result<Vec<_>, std::num::TryFromIntError>>()?;
    let mut chart = ChartBuilder::on(&root)
        .caption(CHART_TITLE, (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800))
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0..(last_index + 1), 0u64..(max_games + max_games / 10 + 1))?;

    let label_count = data.len().clamp(1, MAX_X_LABELS);
    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(label_count)
        .x_label_formatter(&|index| {
            usize::try_from(*index)
                .ok()
                .and_then(|index| data.get(index))
                .map(|week| week.label.clone())
                .unwrap_or_default()
        })
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(LineSeries::new(points.iter().copied(), &BLUE_600))?
        .label("Games")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    chart.draw_series(points.iter().map(|(index, games)| {
        Circle::new((*index, *games), DATA_POINT_RADIUS, BLUE_600.filled())
    }))?;
    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE, &BASE_800))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    root.present()?;
    Ok(())
}
