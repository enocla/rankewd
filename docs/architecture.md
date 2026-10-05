# Architecture

The application is intentionally small, but its dependencies have one-way
boundaries:

```text
main
  -> cli (clap parsing and reporting)
      -> analysis (selection, contract, registry)
          -> dataset (Parquet row access and player aggregation)
          -> plot (shared chart theme)
      -> Parquet / Plotters adapters
```

## Analysis layout

`src/analysis.rs` is the stable analysis boundary. It owns the
`Analysis` contract, `AnalysisInput`, `AnalysisReport`, output location rules,
and the registry returned by `available()`.

Each registered analysis has one implementation file directly under
`src/analysis/`, and every plot has its own analysis so `--analysis` also
selects which chart is drawn:

- `records.rs` reports row counts after the default match filters.
- `games_per_week.rs` owns date projection, ISO-week aggregation, and the
  games-per-week chart.
- `games_per_player.rs` charts games played per player and reports the mean
  and median.
- `games_per_hour.rs` charts matches by the UTC hour they started in, forfeits
  included, and highlights the busiest hour.
- `hour_entropy.rs` bins each player's hour-of-day entropy and charts the
  distribution, forfeits included.
- `rolling_win_duration.rs` charts each eligible player's ten-win rolling mean
  for type-2, non-decayed, non-forfeited wins over season progress.
- `elo_versus_games.rs` charts latest Elo against games played.
- `elo_distribution.rs` bins rated players by latest Elo.
- `elo_versus_win_duration.rs` charts each rated player's final Elo against the
  mean duration of their wins and reports the Pearson correlation.
- `name_underscores_versus_elo.rs` charts a box plot of final Elo per underscore
  count in the player's name.
- `winrate_versus_elo.rs` charts player win rate against latest Elo.
- `winrate_over_time.rs` charts cumulative win rate at every rated, decided
  match.
- `sampled_winrate_over_time.rs` charts the evenly spaced snapshots of a
  reproducible player sample.
- Twenty playerbase plots read the shared two-player match loader:
  `split_evolution.rs`, `win_time_distribution.rs`, `season_retention.rs`,
  `veteran_edge.rs`, `elo_percentiles_by_season.rs`, `elo_calibration.rs`,
  `upsets_by_elo.rs`, `peak_versus_final.rs`, `top_ten_trajectories.rs`,
  `grind_curve.rs`, `streak_winrate.rs`, `session_fatigue.rs`,
  `quit_after_loss.rs`, `weekday_hour_heatmap.rs`,
  `matchmaking_gap_by_hour.rs`, `elo_by_hour.rs`,
  `milestone_lead_winrate.rs`, `deaths_versus_elo.rs`,
  `seed_bastion_win_time.rs`, and `country_elo.rs`. The README lists what each
  one shows.

The playerbase plots share `dataset::load_duels`, which reads every selected
file in parallel with a projection built from each file's own schema, since
older seasons store some columns with other types (season 1 keeps
`changes.eloRate` as a double) or omit them. It keeps non-decayed,
non-forfeited matches between two distinct players, orders them by date and
ID, and records each side's pre-match Elo and change from `changes`, its
country, and, when requested, the first time of each route milestone and its
death count from `timelines`. `dataset::session_slots` splits a player's
matches into sessions separated by 30 idle minutes. Shared line-chart,
heatmap-color, and axis helpers for these plots live in `src/plot.rs`.

There are no analysis-specific style, utility, or output submodules. Shared
Parquet concerns, including the player-season and match-history aggregation the
player analyses read and the per-UTC-hour match counts the two hour-of-day
analyses share, live in `src/dataset.rs`; shared chart typography, axis
padding, log key points, the Pearson correlation used by scatter plots, and
legend components live in `src/plot.rs`.

## Defaults and outputs

The stable default input is `dataset/seasons_10_11.parquet`, but the default
season filter is `11`, so every analysis processes only season 11 unless the
user supplies another selection. This keeps the default input path stable
without silently analyzing both seasons in the merged file.

Curated Parquet files contain only `type = 2` rows. Independently of that data
invariant, every analysis applies the default `decayed = false` and
`forfeited = false` row filters before producing records, aggregates, or plots.
The two hour-of-day analyses are the one exception: `games-per-hour` and
`hour-entropy` describe when matches are played rather than how they ended, so
they keep forfeited matches, drop only decayed rows, and say so in their
reports.

All generated artifacts default to `output/`. The directory is ignored, and
`--output` relocates the generated charts rather than sending them back into
`dataset/`.

## Input and output flow

1. `clap` parses the command line and applies one consistent set of defaults.
2. The registry validates and selects the requested analysis before dataset
   work begins.
3. `select_files` resolves a file or directory into `DatasetFile` values.
4. `AnalysisInput` carries the files, optional row-level season filter, and
   output location.
5. Analyses read through `dataset`, which applies the default match filters
   apart from the documented hour-of-day exception; analyses return an
   `AnalysisReport` and do not print directly.
6. The CLI prints the report.

A season filter is applied to the `season` column even when a single merged
Parquet file is selected. This prevents a file-level shortcut from silently
including unrequested seasons.

Generated files default to `output/`. `OutputLocation` keeps every chart of a
run in one place: a single-output analysis uses the supplied path verbatim, and
a per-season analysis writes its season-suffixed PNG next to it. Analyses do
not delete old chart files, so a run never removes an artifact it did not
create.

## Data contracts

Parquet is the runtime input format. The Serde types in `src/schema.rs` remain
the public compatibility model for the original JSON Lines export; they are not
loaded for every analysis because most analyses only need a small projection.

Chart typography and colors are centralized in `src/plot.rs`. Renderers use
Lilex Nerd Font, the Flexoki palette, 1400×800 canvases, large axis/tick text,
upper-right legends, and the required label areas.

Automated tests are intentionally not part of this project. Changes are
validated with formatting, compilation, strict Clippy, and targeted CLI smoke
runs against the local dataset.
