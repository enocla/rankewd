# Architecture

The application is intentionally small, but its dependencies have one-way
boundaries:

```text
main
  -> cli (clap parsing and reporting)
      -> analysis (selection, contract, registry)
          -> dataset (Parquet row access)
          -> plot (shared chart theme)
      -> Parquet / Arrow / Plotters adapters
```

## Analysis layout

`src/analysis.rs` is the stable analysis boundary. It owns the
`Analysis` contract, `AnalysisInput`, `AnalysisReport`, output location rules,
and the registry returned by `available()`.

Each registered analysis has one implementation file directly under
`src/analysis/`:

- `records.rs` reports Parquet metadata counts.
- `games_per_week.rs` owns date projection, ISO-week aggregation, and its
  chart.
- `elo_versus_games.rs` owns player aggregation, temporal points, the player
  Parquet export, and its four charts.

There are no analysis-specific style, utility, or output submodules. Shared
Parquet and chart concerns live at the crate boundary instead.

## Defaults and outputs

The stable default input is `dataset/seasons_10_11.parquet`, but the default
season filter is `11`, so every analysis processes only season 11 unless the
user supplies another selection. This keeps the default input path stable
without silently analyzing both seasons in the merged file.

All generated artifacts default to `output/`. The directory is ignored, and
`--output` relocates the complete output family rather than sending derived
data back into `dataset/`.

## Input and output flow

1. `clap` parses the command line and applies one consistent set of defaults.
2. The registry validates and selects the requested analysis before dataset
   work begins.
3. `select_files` resolves a file or directory into `DatasetFile` values.
4. `AnalysisInput` carries the files, optional row-level season filter, and
   output location.
5. Analyses read through `dataset`, return an `AnalysisReport`, and do not
   print directly.
6. The CLI prints the report.

A season filter is applied to the `season` column even when a single merged
Parquet file is selected. This prevents a file-level shortcut from silently
including unrequested seasons.

Generated files default to `output/`. `OutputLocation` keeps the multi-file
Elo output family together, including `player_elos.parquet`; the old
hard-coded `dataset/player_elos.parquet` destination is not used. Analyses do
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
