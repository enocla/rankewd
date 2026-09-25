use super::{Analysis, SeasonFile};
use parquet::file::reader::{FileReader, SerializedFileReader};
use std::error::Error;
use std::fs::File;

/// Reports the number of records stored in each season file.
pub struct RecordCount;

impl Analysis for RecordCount {
    fn name(&self) -> &'static str {
        "records"
    }

    fn run(&self, files: &[SeasonFile]) -> Result<(), Box<dyn Error>> {
        for season_file in files {
            let reader = SerializedFileReader::new(File::open(&season_file.path)?)?;
            let record_count = reader.metadata().file_metadata().num_rows();
            println!(
                "Season {}: {record_count} records ({})",
                season_file.season,
                season_file.path.display()
            );
        }
        Ok(())
    }
}
