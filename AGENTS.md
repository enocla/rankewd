# Repository conventions

## Analyses

- Put each analysis type in its own Rust source file under `src/analysis/`.
- Keep each analysis implementation complete in that one file; do not add analysis-specific style, utility, data, output, or chart submodules.
- Give every plot its own file and its own `--analysis` name so a run renders one plot.
- Register each analysis in `analysis::available()` so the CLI can select it.
- Name each analysis with a concise, stable CLI name through the `Analysis` trait.
- Keep data shared by several analyses in `src/dataset.rs` and chart components shared by several plots in `src/plot.rs`.

## Defaults and outputs

- Use `dataset/seasons_10_11.parquet` as the stable default input and filter the default analysis scope to season 11.
- Keep curated Parquet data limited to `type = 2`; remove rows of every other match type from the data files.
- Filter every analysis to `decayed = false` and `forfeited = false` by default. Only rows with both values false may contribute to records, aggregates, or plots.
- Keep forfeited matches only where an analysis reports when matches were played rather than how they ended. `games-per-hour` and `hour-entropy` are the sanctioned exceptions: they require `decayed = false` alone and must state the forfeited share in their reports.
- The explicitly requested forfeit/recovery research analyses (`opponent-after-death`, `opponent-after-death-winrate`, `opponent-death-timing`, `recovery-*`, `death-reset-exclusions`, `death-rank-deficit`, and `forfeit-*`) also keep real forfeited matches, exclude decay, and report event attribution and missing cases. Existing analyses retain their default filters.
- The main research scope is seasons 10 and 11; keep all-season comparisons separately labeled. The unrelated CLI default scope remains season 11.
- For this research, ignore world-reset events. Recovery exposures are first ordinary deaths; continuation means no own forfeit, without a later-split requirement. Keep intentional spawnpoint deaths excluded.
- Put every generated artifact under `output/` by default; never write runtime output to the repository root or `dataset/`.
- Keep `output/` ignored and do not commit generated PNG artifacts.
- Treat `--output` as an explicit relocation of the complete output family: the exact path for single-output analyses, the containing directory for per-season analyses.
- Do not delete files that were not produced by the current run.

## CLI

- Define the command line declaratively with `clap` in `src/cli.rs`; do not add a hand-written argument parser.
- Keep analysis lookup and user-facing reporting in the CLI layer; analyses return reports instead of printing directly.

## Report language

- Use ASD-STE100 writing rules for research titles, labels, legends, and reports.
- Define technical terms and use the same term for the same meaning.
- Use direct phrases such as “at time of death.”

## Typography

- Use **SF Pro Text** as the font for all user-facing text and generated visuals, including chart titles, labels, legends, reports, and UI text.
- Use the **Flexoki** color palette for charts and generated visuals. Use its warm paper and base colors for backgrounds, text, axes, and grids, and its accent colors for data series. Palette source: [Flexoki by Steph Ango](https://stephango.com/flexoki).
- For 2800×1600 charts, use 80-point axis descriptions and 64-point tick labels, with enough label-area space and outer margin to keep the larger text clear.
- Use a 96-point chart title, place the legend in the upper-right corner, and leave generous room to the right of the rotated y-axis description (at least 320 pixels of y-label area on a 2800×1600 chart).
- Use the repository's Nix development shell through direnv (`.envrc`); `nix develop` is the manual fallback.

## Testing

- This project intentionally does not maintain automated tests. Do not add unit, integration, or test-only modules.
- Validate changes with `cargo fmt --all -- --check`, `cargo check --all-targets`, strict Clippy, and targeted CLI smoke runs against the local dataset.
