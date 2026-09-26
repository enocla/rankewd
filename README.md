# rankewd

`rankewd` reads Rankewd Parquet match data and runs focused analyses over the
selected seasons. It is both a small command-line application and a library for
the source-record schema exposed by [`rankewd::schema`](src/schema.rs). Curated
datasets contain only `type = 2` matches, and every analysis excludes decayed
and forfeited matches by default. The two hour-of-day analyses are the
documented exception: they keep forfeited matches and drop only decayed ones.

## Repository layout

- `src/main.rs` is a thin process entrypoint.
- `src/cli.rs` owns the declarative [`clap`](https://docs.rs/clap/) command
  definition and application orchestration.
- `src/analysis.rs` contains the analysis contract, registry, input model, and
  output policy.
- Every plot has its own file under `src/analysis/` and its own
  `--analysis` name: `records.rs` (`records`), `games_per_week.rs`
  (`games-per-week`), `games_per_player.rs` (`games-per-player`),
  `games_per_hour.rs` (`games-per-hour`), `hour_entropy.rs` (`hour-entropy`),
  `rolling_win_duration.rs` (`rolling-win-duration`),
  `elo_versus_games.rs` (`elo-versus-games`), `elo_distribution.rs`
  (`elo-distribution`), `elo_versus_win_duration.rs`
  (`elo-versus-win-duration`), `name_underscores_versus_elo.rs`
  (`name-underscores-versus-elo`), `winrate_versus_elo.rs`
  (`winrate-versus-elo`), `winrate_over_time.rs` (`winrate-over-time`), and
  `sampled_winrate_over_time.rs` (`sampled-winrate-over-time`).
  Analysis-specific code stays in those files.
- `src/dataset.rs` is the shared Parquet access boundary, including the player
  and match-history aggregation shared by the player analyses and the UTC
  hour-of-day counts shared by the two hour analyses.
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

Each plot is selected by its own `--analysis` value. For example,
`--analysis rolling-win-duration` plots one line per player with more than 100
games, aligned by season progress, with the mean duration of that player's ten
most recent type-2, non-decayed, non-forfeited wins in seconds. Lower values
indicate improvement; the chart uses a logarithmic duration axis and overlays
the mean player trend.
`--analysis sampled-winrate-over-time` plots a reproducible random sample of up
to 100 eligible players, with five cumulative win-rate snapshots selected
nearest the start, quarter, midpoint, three-quarter, and end of each season.
`--analysis elo-versus-win-duration` scatters rated players by their final Elo
and the mean duration in seconds of their type-2 wins, and prints the Pearson
correlation of the two. `--analysis name-underscores-versus-elo` draws a box
plot of final Elo grouped by how many underscores the player's name held at
their final rating, and reports each group's size, median, and outlier count.
Both plot analyses report their key numbers in the chart title as well as in the
CLI report.
`--analysis games-per-hour` bars the non-decayed matches of each season by the
UTC hour their match started in and highlights the busiest hour; it is the one
analysis that plots the hour of a match rather than a player's results, so it
keeps forfeited matches and reports how many of them it counted.
`--analysis hour-entropy` bins the same hour-of-day counts per player into
0.25-bit Shannon entropy bins, where 0 bits means every match fell in one UTC
hour and `log2(24) = 4.58` bits means an even spread across the day. It
requires at least 100 matches per player, because below that the entropy mostly
measures how few games a player has, and it reports the median entropy, the
narrowest player, and the share of players within a bin of the maximum.

For `games-per-week`, which writes a single chart, `--output` is the exact PNG
path. The per-season analyses write one PNG per season into the directory that
holds `--output`, keeping the documented file names such as
`elo_vs_games_season_11.png`.
