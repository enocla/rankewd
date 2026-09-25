use super::{Analysis, DatasetFile};
use chrono::{DateTime, Datelike, Duration, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use parquet::schema::parser::parse_message_type;
mod style;
mod utils;

use self::style::OUTPUT_FILE;
use self::utils::{draw_chart, iso_week_monday};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io;
use std::path::Path;

/// Plots the number of recorded games in each UTC calendar week.
pub struct GamesPerWeek;

impl Analysis for GamesPerWeek {
    fn name(&self) -> &'static str {
        "games-per-week"
    }

    fn run(&self, files: &[DatasetFile], output: Option<&Path>) -> Result<(), Box<dyn Error>> {
        let mut games_by_week = BTreeMap::<(i32, u32), u64>::new();

        for dataset_file in files {
            let reader = SerializedFileReader::new(File::open(&dataset_file.path)?)?;
            let date_projection = parse_message_type("message root { optional int64 date; }")?;
            for row in reader.get_row_iter(Some(date_projection.clone()))? {
                let row = row?;
                let timestamp = row
                    .get_column_iter()
                    .find_map(|(name, value)| {
                        if name == "date" {
                            Some(match value {
                                Field::Long(value) => Ok(*value),
                                Field::Int(value) => Ok(i64::from(*value)),
                                other => Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("expected integer date column, found {other:?}"),
                                )),
                            })
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "missing date column")
                    })??;
                let date = DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "date is outside supported range",
                    )
                })?;
                let iso_week = date.iso_week();
                *games_by_week
                    .entry((iso_week.year(), iso_week.week()))
                    .or_default() += 1;
            }
        }

        if games_by_week.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no games found in selected files",
            )
            .into());
        }

        let first = *games_by_week.keys().next().expect("non-empty map");
        let last = *games_by_week.keys().next_back().expect("non-empty map");
        let mut week_start = iso_week_monday(first.0, first.1)?;
        let last_week_start = iso_week_monday(last.0, last.1)?;
        let mut weekly_counts = Vec::new();
        while week_start <= last_week_start {
            let iso_week = week_start.iso_week();
            weekly_counts.push((
                format!("{}-W{:02}", iso_week.year(), iso_week.week()),
                *games_by_week
                    .get(&(iso_week.year(), iso_week.week()))
                    .unwrap_or(&0),
            ));
            week_start += Duration::weeks(1);
        }

        let output = output.unwrap_or_else(|| Path::new(OUTPUT_FILE));
        draw_chart(output, &weekly_counts)?;
        println!("Wrote {} ({} weeks)", output.display(), weekly_counts.len());
        Ok(())
    }
}
