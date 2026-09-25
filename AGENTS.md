# Repository conventions

## Analyses

- Put each analysis type in its own Rust source file under `src/analysis/`.
- Keep shared analysis interfaces and season-file types in `src/analysis.rs`.
- Register each analysis in `analysis::available()` so the CLI can select it.
- Name each analysis with a concise, stable CLI name through the `Analysis` trait.
