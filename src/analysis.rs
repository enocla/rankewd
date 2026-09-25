use std::error::Error;
use std::path::{Path, PathBuf};

mod elo_versus_games;
mod games_per_week;
mod records;

/// One Parquet dataset selected for analysis.
pub struct DatasetFile {
    pub label: String,
    pub path: PathBuf,
}

/// An analysis that can be run over the selected season files.
pub trait Analysis {
    fn name(&self) -> &'static str;
    fn run(&self, files: &[DatasetFile], output: Option<&Path>) -> Result<(), Box<dyn Error>>;
}

/// Available analyses. Add new implementations here to expose them to the CLI.
pub fn available() -> Vec<Box<dyn Analysis>> {
    vec![
        Box::new(records::RecordCount),
        Box::new(games_per_week::GamesPerWeek),
        Box::new(elo_versus_games::EloVersusGames),
    ]
}
