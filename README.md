# rankewd

`rankewd` reads Rankewd Parquet match data and runs focused analyses over the
selected seasons. It is both a small command-line application and a library for
the source-record schema exposed by [`rankewd::schema`](src/schema.rs).

## Repository layout

- `src/main.rs` is a thin process entrypoint.
- `src/cli.rs` owns the declarative [`clap`](https://docs.rs/clap/) command
  definition and application orchestration.
- `src/analysis.rs` contains the analysis contract, registry, input model, and
  output policy.
- Every plot has its own file under `src/analysis/` and its own
  `--analysis` name: `records.rs` (`records`), `games_per_week.rs`
  (`games-per-week`), `games_per_player.rs` (`games-per-player`),
  `elo_versus_games.rs` (`elo-versus-games`), `elo_distribution.rs`
  (`elo-distribution`), `winrate_versus_elo.rs`
  (`winrate-versus-elo`), `winrate_over_time.rs` (`winrate-over-time`), and
  `sampled_winrate_over_time.rs` (`sampled-winrate-over-time`).
  Analysis-specific code stays in those files.
- `src/dataset.rs` is the shared Parquet access boundary, including the player
  and match-history aggregation shared by the player analyses.
- `src/plot.rs` is the shared chart theme, axis helpers, and reusable legend
  components.
- `dataset/` is ignored local input data.
- `output/` is the default location for all generated artifacts and is ignored by Git.
- `docs/` documents the data contract and architecture.

## Development

The repository uses the Nix development shell through direnv:

```sh
direnv allow
```

If direnv is not available, use `nix develop`. The shell supplies Rust,
fontconfig, and **Lilex Nerd Font** for chart rendering.

Useful commands:

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo run -- --help
```

This project intentionally does not maintain automated tests; use the checks
above and targeted CLI smoke runs while working with local data.

## Usage

The default command uses the stable merged Parquet file but restricts analysis
to season 11:

```sh
cargo run -- --analysis records
```

Select all season files explicitly:

```sh
cargo run -- --dataset dataset --seasons all --analysis records
```

Run an analysis and keep generated files in the default `output/` directory:

```sh
cargo run -- --dataset dataset --seasons all --analysis elo-versus-games
```

Each plot is selected by its own `--analysis` value, for example
`--analysis games-per-player`, `--analysis elo-distribution`,
`--analysis winrate-versus-elo`, `--analysis winrate-over-time`, or
`--analysis sampled-winrate-over-time`. The
last one plots a reproducible random sample of up to 100 eligible players, with
five cumulative win-rate snapshots selected nearest the start, quarter,
midpoint, three-quarter, and end of each season.

For `games-per-week`, which writes a single chart, `--output` is the exact PNG
path. The per-season analyses write one PNG per season into the directory that
holds `--output`, keeping the documented file names such as
`elo_vs_games_season_11.png`.
