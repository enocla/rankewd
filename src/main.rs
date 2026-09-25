mod analysis;

use analysis::SeasonFile;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DEFAULT_SEASONS: &[u32] = &[10, 11];

struct Options {
    dataset: PathBuf,
    seasons: Vec<u32>,
    analysis: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = match parse_options(std::env::args_os().skip(1))? {
        Some(options) => options,
        None => return Ok(()),
    };

    let files = find_season_files(&options.dataset, &options.seasons)?;
    let analyses = analysis::available();
    let selected = analyses
        .iter()
        .find(|analysis| analysis.name() == options.analysis)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "unknown analysis '{}'; available analyses: {}",
                    options.analysis,
                    analyses
                        .iter()
                        .map(|analysis| analysis.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;

    selected.run(&files)
}

fn parse_options(args: impl Iterator<Item = OsString>) -> Result<Option<Options>, Box<dyn Error>> {
    let mut dataset = PathBuf::from("dataset");
    let mut seasons = DEFAULT_SEASONS.to_vec();
    let mut analysis = String::from("records");
    let mut args = args;

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                print_help();
                return Ok(None);
            }
            Some("--dataset") => {
                dataset = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| invalid_arg("--dataset requires a directory"))?;
            }
            Some("--seasons") => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_arg("--seasons requires a comma-separated list"))?;
                seasons = value
                    .to_str()
                    .ok_or_else(|| invalid_arg("season list must be valid UTF-8"))?
                    .split(',')
                    .map(|season| {
                        season
                            .parse::<u32>()
                            .map_err(|_| invalid_arg("seasons must be positive integers"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if seasons.is_empty() || seasons.contains(&0) {
                    return Err(invalid_arg("seasons must be positive integers").into());
                }
                seasons.sort_unstable();
                seasons.dedup();
            }
            Some("--analysis") => {
                analysis = args
                    .next()
                    .ok_or_else(|| invalid_arg("--analysis requires a name"))?
                    .into_string()
                    .map_err(|_| invalid_arg("analysis name must be valid UTF-8"))?;
            }
            _ => {
                return Err(
                    invalid_arg(&format!("unknown argument: {}", arg.to_string_lossy())).into(),
                );
            }
        }
    }

    Ok(Some(Options {
        dataset,
        seasons,
        analysis,
    }))
}

fn find_season_files(dataset: &Path, seasons: &[u32]) -> Result<Vec<SeasonFile>, Box<dyn Error>> {
    let mut paths = Vec::new();
    collect_parquet_files(dataset, &mut paths)?;

    let mut files = Vec::new();
    for season in seasons {
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

        files.extend(matching.into_iter().map(|path| SeasonFile {
            season: *season,
            path,
        }));
    }

    Ok(files)
}

fn collect_parquet_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_parquet_files(&entry_path, files)?;
        } else if entry_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("parquet"))
        {
            files.push(entry_path);
        }
    }
    Ok(())
}

fn invalid_arg(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_owned())
}

fn print_help() {
    println!(
        "Usage: rankewd [--dataset DIR] [--seasons N[,N...]] [--analysis NAME]\n\
         Defaults: --dataset dataset --seasons 10,11 --analysis records\n\
         Available analyses: {}",
        analysis::available()
            .iter()
            .map(|analysis| analysis.name())
            .collect::<Vec<_>>()
            .join(", ")
    );
}
