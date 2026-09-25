use std::collections::BTreeSet;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod elo_distribution;
mod elo_versus_games;
mod elo_versus_win_duration;
mod games_per_player;
mod games_per_week;
mod name_length_versus_elo;
mod records;
mod rolling_win_duration;
mod sampled_winrate_over_time;
mod winrate_over_time;
mod winrate_versus_elo;

/// Directory used for generated artifacts when `--output` is omitted.
pub const DEFAULT_OUTPUT_DIRECTORY: &str = "output";

/// One Parquet dataset selected for analysis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetFile {
    pub label: String,
    pub path: PathBuf,
}

/// Resolves analysis output while keeping every generated artifact in one place.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutputLocation {
    path: Option<PathBuf>,
}

impl OutputLocation {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// Resolves a single explicit output path, or uses the supplied default.
    pub fn resolve(&self, default: impl Into<PathBuf>) -> PathBuf {
        self.path.clone().unwrap_or_else(|| default.into())
    }

    /// Places a generated file in the selected output directory.
    pub fn sibling(&self, file_name: &str) -> PathBuf {
        match &self.path {
            Some(path) => output_parent(path).join(file_name),
            None => Path::new(DEFAULT_OUTPUT_DIRECTORY).join(file_name),
        }
    }
}

/// Immutable input supplied to an analysis.
pub struct AnalysisInput<'a> {
    files: &'a [DatasetFile],
    seasons: Option<&'a [u32]>,
    output: OutputLocation,
}

impl<'a> AnalysisInput<'a> {
    pub fn new(
        files: &'a [DatasetFile],
        seasons: Option<&'a [u32]>,
        output: OutputLocation,
    ) -> Self {
        Self {
            files,
            seasons,
            output,
        }
    }

    pub fn files(&self) -> &'a [DatasetFile] {
        self.files
    }

    pub fn includes_season(&self, season: u32) -> bool {
        self.seasons.is_none_or(|seasons| seasons.contains(&season))
    }

    pub fn seasons(&self) -> Option<&[u32]> {
        self.seasons
    }

    pub fn output(&self) -> &OutputLocation {
        &self.output
    }
}

/// User-facing completion messages produced by an analysis.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnalysisReport {
    lines: Vec<String>,
}

impl AnalysisReport {
    pub fn line(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

/// An analysis that can be run over selected Parquet datasets. Each registered
/// analysis renders one plot, so `--analysis` also selects which chart is drawn.
pub trait Analysis {
    /// Concise, stable CLI name.
    fn name(&self) -> &'static str;

    /// Executes the analysis and returns messages for the CLI to print.
    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>>;
}

/// Available analyses. Add new implementations here to expose them to the CLI.
pub fn available() -> Vec<Box<dyn Analysis>> {
    vec![
        Box::new(records::RecordCount),
        Box::new(games_per_week::GamesPerWeek),
        Box::new(games_per_player::GamesPerPlayer),
        Box::new(rolling_win_duration::RollingWinDuration),
        Box::new(elo_versus_games::EloVersusGames),
        Box::new(name_length_versus_elo::NameLengthVersusElo),
        Box::new(elo_versus_win_duration::EloVersusWinDuration),
        Box::new(elo_distribution::EloDistribution),
        Box::new(winrate_versus_elo::WinrateVersusElo),
        Box::new(winrate_over_time::WinrateOverTime),
        Box::new(sampled_winrate_over_time::SampledWinrateOverTime),
    ]
}

/// Resolves a dataset path and optional season filter into analysis inputs.
pub fn select_files(
    dataset: &Path,
    seasons: Option<&[u32]>,
) -> Result<Vec<DatasetFile>, Box<dyn Error>> {
    if dataset.is_file() {
        if !is_parquet_file(dataset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a Parquet file", dataset.display()),
            )
            .into());
        }

        let label = match seasons {
            Some(seasons) if seasons.len() == 1 => format!("Season {}", seasons[0]),
            _ => dataset
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Dataset")
                .to_owned(),
        };
        return Ok(vec![DatasetFile {
            label,
            path: dataset.to_path_buf(),
        }]);
    }

    if !dataset.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("dataset path does not exist: {}", dataset.display()),
        )
        .into());
    }

    let mut paths = Vec::new();
    collect_parquet_files(dataset, &mut paths)?;

    let requested_seasons = match seasons {
        Some(seasons) => seasons.iter().copied().collect::<BTreeSet<_>>(),
        None => paths
            .iter()
            .filter_map(|path| season_from_path(path))
            .collect(),
    };

    if requested_seasons.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no files named season_N_*.parquet found in {}",
                dataset.display()
            ),
        )
        .into());
    }

    let mut files = Vec::new();
    for season in requested_seasons {
        let prefix = format!("season_{season}_");
        let mut matching: Vec<_> = paths
            .iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
            })
            .cloned()
            .collect();
        matching.sort();

        if matching.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "no Parquet file found for season {season} in {}",
                    dataset.display()
                ),
            )
            .into());
        }

        files.extend(matching.into_iter().map(|path| DatasetFile {
            label: format!("Season {season}"),
            path,
        }));
    }

    Ok(files)
}

fn season_from_path(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let remainder = name.strip_prefix("season_")?;
    let (season, _) = remainder.split_once('_')?;
    season.parse().ok()
}

fn collect_parquet_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), io::Error> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_parquet_files(&entry_path, files)?;
        } else if is_parquet_file(&entry_path) {
            files.push(entry_path);
        }
    }
    Ok(())
}

fn is_parquet_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("parquet"))
}

fn output_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(""))
}
