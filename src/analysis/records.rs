use super::{Analysis, DatasetFile};
use parquet::file::reader::{FileReader, SerializedFileReader};
use std::error::Error;
use std::fs::File;
use std::path::Path;

/// Reports the number of records stored in each season file.
pub struct RecordCount;

impl Analysis for RecordCount {
    fn name(&self) -> &'static str {
        "records"
    }

    fn run(&self, files: &[DatasetFile], _output: Option<&Path>) -> Result<(), Box<dyn Error>> {
        for dataset_file in files {
            let reader = SerializedFileReader::new(File::open(&dataset_file.path)?)?;
            let record_count = reader.metadata().file_metadata().num_rows();
            println!(
                "{}: {record_count} records ({})",
                dataset_file.label,
                dataset_file.path.display()
            );
        }
        Ok(())
    }
}
