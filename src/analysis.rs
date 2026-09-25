use std::error::Error;
use std::path::PathBuf;

mod records;

/// One Parquet dataset selected for analysis.
pub struct DatasetFile {
    pub label: String,
    pub path: PathBuf,
}

/// An analysis that can be run over the selected season files.
pub trait Analysis {
    fn name(&self) -> &'static str;
    fn run(&self, files: &[DatasetFile]) -> Result<(), Box<dyn Error>>;
}

/// Available analyses. Add new implementations here to expose them to the CLI.
pub fn available() -> Vec<Box<dyn Analysis>> {
    vec![Box::new(records::RecordCount)]
}
