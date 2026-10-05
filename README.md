# rankewd

`rankewd` reads Rankewd Parquet match data and runs focused analyses over the
selected seasons. It is both a small command-line application and a library for
the source-record schema exposed by [`rankewd::schema`](src/schema.rs). Curated
datasets contain only `type = 2` matches, and every analysis excludes decayed
and forfeited matches by default. The two hour-of-day analyses and the explicitly requested forfeit/recovery
research are documented exceptions: they retain forfeits and drop decay.

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
  `sampled_winrate_over_time.rs` (`sampled-winrate-over-time`), plus the
  twenty playerbase plots listed under [Playerbase plots](#playerbase-plots).
  Analysis-specific code stays in those files.
- `src/dataset.rs` is the shared Parquet access boundary, including the player
  and match-history aggregation shared by the player analyses, the UTC
  hour-of-day counts shared by the two hour analyses, and the two-player match
  loader (`load_duels`) and session splitting shared by the playerbase plots.
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
and fontconfig. Chart rendering uses the installed **SF Pro Text** family.

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

## Playerbase plots

Twenty analyses look for surprises in the playerbase and how it changes within
and across seasons. They are meant to be run over every season:

```sh
cargo run --release -- --dataset dataset --seasons all --analysis elo-calibration
```

Each writes one chart to `output/<name>.png` (`--output` is the exact path),
except `peak-versus-final` and `top-ten-trajectories`, which write one chart
per season. Elo values come from `changes[].eloRate`, the rating a player
carried into the match; `players[].eloRate` is a stale profile snapshot and is
not used. Titles carry the headline number, and the CLI report lists the values
behind every point.

| `--analysis` | What it shows |
| --- | --- |
| `split-evolution` | Median winner split at each route milestone (Nether, bastion, fortress, blind travel, stronghold, End) and finish, per season. |
| `win-time-distribution` | Distribution of winning times, one curve per season. |
| `season-retention` | Heatmap of the share of one season's players (10+ matches) who also played each other season. |
| `veteran-edge` | Win rate of returning players against season debutants within 50 Elo of them. |
| `elo-percentiles-by-season` | Final-Elo percentiles (p10–p99 and the top player) per season. |
| `elo-calibration` | Higher-rated player's actual win rate against the Elo formula, by rating gap. |
| `upsets-by-elo` | Underdog win rate at a 100–200 gap, by the pair's mean Elo. |
| `peak-versus-final` | Per season: every player's peak Elo against how far they finished below it. |
| `top-ten-trajectories` | Per season: daily closing Elo of the final top ten. |
| `grind-curve` | Mean Elo gained over the first 300 rated matches by players who played that many. |
| `streak-winrate` | Win rate above Elo expectation by the win or loss streak a player brings in. |
| `session-fatigue` | Win rate above expectation by match number within a session. |
| `quit-after-loss` | How much likelier a session is to end after a loss than a win, by Elo. |
| `weekday-hour-heatmap` | Matches per UTC weekday and hour, relative to the weekly mean hour. |
| `matchmaking-gap-by-hour` | Median, p75, and p90 pre-match Elo gap by UTC hour. |
| `elo-by-hour` | Elo percentiles of the players queuing at each UTC hour. |
| `milestone-lead-winrate` | Win rate of the player who reached each milestone first, by size of the lead. |
| `deaths-versus-elo` | Timeline deaths per match for winners and losers, by Elo. |
| `seed-bastion-win-time` | Median winning time for every overworld seed type and bastion pairing. |
| `country-elo` | Final-Elo median and IQR for the 16 countries with the most player-seasons. |

A session ends when a player starts no match within 30 minutes of their
previous match ending. Win rates "vs Elo expectation" subtract the logistic
400-point Elo prediction from the outcome, so they isolate what the rating does
not already explain.


## Forfeit research

Run the main study on seasons 10 and 11:

```sh
bash scripts/run-forfeit-research.sh
```

The script writes 30 main graphs restricted to seasons 10–11, one separately
labeled all-season recovery comparison, CSVs,
individual CLI reports and a shared measurement cache to `output/research/`.
It also assembles an illustrated report in `report.md` and `index.html`,
with every relevant graph beside its discussion, using `scripts/build_research_report.py`. Set
`RANKEWD_RESEARCH_OUTPUT` to relocate that complete family. The findings builder
requires a main measurement cache restricted to seasons 10 and 11. The script also generates a separate all-season report under
`output/research/all_seasons/`, including three partial-season-12 figures. Titles, labels, legends, and reports use ASD-STE100 writing rules and defined
technical terms. All recovery and `opponent-*` plots restrict
their main cohort to first ordinary deaths with no recorded own forfeit, without
requiring another split. Observation ends at the first completion or forfeit,
even if later events are present in the timeline. World resets are ignored in
every research measurement. The Elo-efficiency
comparison also starts at death and retains forfeiting as the comparison action;
habit analyses retain rated matches to measure forfeit rates. Both opponent-event
curves use the same cohort; they do not compare forfeiting against continued play. Each
plot also has its own `--analysis` name:

- `recovery-winrate`
- `recovery-seasons`
- `recovery-stage`
- `recovery-deficit`
- `recovery-opponent`
- `opponent-after-death`
- `opponent-after-death-winrate`
- `opponent-death-timing`
- `death-rank-deficit`
- `recovery-time`
- `death-reset-exclusions`
- `recovery-outcomes`
- `forfeit-coverage`
- `forfeit-rating`
- `forfeit-winrate`
- `forfeit-residual`
- `forfeit-pace`
- `forfeit-completion`
- `forfeit-improvement`
- `forfeit-hour-improvement`
- `forfeit-pace-improvement`
- `recovery-improvement`
- `forfeit-death-improvement`
- `recovery-pace-improvement`
- `forfeit-retention`
- `forfeit-cross-season-pace`
- `forfeit-cross-season-rating`
- `recovery-elo`
- `recovery-efficiency`
- `forfeit-calibration`

These analyses deliberately include real forfeited matches, as required for
recovery and quit-habit research; existing analyses keep their default filters.
See [the dataset documentation](docs/dataset-format.md#forfeit-and-recovery-research)
for definitions, eligibility, uncertainty and observational limits, and
[JSONL conversion](docs/dataset-format.md#converting-source-jsonl-exports) for
importing missing seasons.
