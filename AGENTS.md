# Repository conventions

## Analyses

- Put each analysis type in its own Rust source file under `src/analysis/`.
- Keep shared analysis interfaces and season-file types in `src/analysis.rs`.
- Register each analysis in `analysis::available()` so the CLI can select it.
- Name each analysis with a concise, stable CLI name through the `Analysis` trait.

## Typography

- Use **Lilex Nerd Font** as the font for all user-facing text and generated visuals, including chart titles, labels, legends, reports, and UI text.
- Use the **Flexoki** color palette for charts and generated visuals. Use its warm paper and base colors for backgrounds, text, axes, and grids, and its accent colors for data series. Palette source: [Flexoki by Steph Ango](https://stephango.com/flexoki).
- For 1400×800 charts, use 40-point axis descriptions and 32-point tick labels, with enough label-area space and outer margin to keep the larger text clear.
- Use a 48-point chart title, place the legend in the upper-right corner, and leave generous room to the right of the rotated y-axis description (at least 160 pixels of y-label area on a 1400×800 chart).
- Use the repository's Nix development shell through direnv (`.envrc`); `nix develop` is the manual fallback.
