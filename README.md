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
- `src/analysis/records.rs`, `games_per_week.rs`, and
  `elo_versus_games.rs` are the complete implementations for the registered
  analysis types. Analysis-specific code stays in those files.
- `src/dataset.rs` is the shared Parquet access boundary.
- `src/plot.rs` is the shared chart theme and filesystem helper.
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

For a single-output analysis, `--output` is the exact PNG path. For
`elo-versus-games`, it is an output base: the parent directory receives the
per-season charts and `player_elos.parquet`, while the supplied stem is used
for the primary Elo-versus-games chart.
