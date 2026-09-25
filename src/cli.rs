use crate::analysis::{AnalysisInput, OutputLocation, available, select_files};
use clap::Parser;
use std::error::Error;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "rankewd",
    version,
    about = "Analyze Rankewd Parquet datasets",
    after_help = "Available analyses: records, games-per-week, games-per-player, rolling-win-duration, \
elo-versus-games, name-length-versus-elo, elo-versus-win-duration, elo-distribution, winrate-versus-elo, winrate-over-time, \
sampled-winrate-over-time\n\nEach analysis runs on its own; the plot analyses write one PNG per season. All analyses exclude decayed and forfeited matches by default.\n\nUse --dataset dataset --seasons all to select every season_N_*.parquet file."
)]
struct Cli {
    /// Parquet file or directory to analyze.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "dataset/seasons_10_11.parquet"
    )]
    dataset: PathBuf,

    /// Comma-separated positive season numbers, or `all`.
    #[arg(
        long,
        value_name = "SEASONS",
        default_value = "11",
        value_parser = parse_seasons
    )]
    seasons: SeasonSelection,

    /// Analysis to run. Each plot has its own analysis name.
    #[arg(long, value_name = "NAME", default_value = "records")]
    analysis: String,

    /// Output file for a single-output analysis, or the output directory for
    /// per-season analyses.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct SeasonSelection(Option<Vec<u32>>);

/// Parses CLI arguments, selects an analysis, and prints its report.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse_from(std::iter::once(OsString::from("rankewd")).chain(args));
    let analyses = available();
    let selected = analyses
        .iter()
        .find(|analysis| analysis.name() == cli.analysis)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "unknown analysis '{}'; available analyses: {}",
                    cli.analysis,
                    analyses
                        .iter()
                        .map(|analysis| analysis.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;

    let files = select_files(&cli.dataset, cli.seasons.0.as_deref())?;
    let input = AnalysisInput::new(
        &files,
        cli.seasons.0.as_deref(),
        OutputLocation::new(cli.output),
    );
    let report = selected.run(&input)?;
    for line in report.lines() {
        println!("{line}");
    }

    Ok(())
}

fn parse_seasons(value: &str) -> Result<SeasonSelection, String> {
    if value.eq_ignore_ascii_case("all") {
        return Ok(SeasonSelection(None));
    }

    let mut seasons = value
        .split(',')
        .map(|season| {
            season
                .parse::<u32>()
                .map_err(|_| "seasons must be positive integers or 'all'".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if seasons.contains(&0) {
        return Err("seasons must be positive integers or 'all'".to_owned());
    }
    seasons.sort_unstable();
    seasons.dedup();
    Ok(SeasonSelection(Some(seasons)))
}
