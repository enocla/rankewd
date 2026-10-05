#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
research_dir="${RANKEWD_RESEARCH_OUTPUT:-output/research}"
mkdir -p "$research_dir"
direnv exec . cargo build --release
analyses=(
  recovery-winrate
  recovery-seasons
  recovery-stage
  recovery-deficit
  recovery-opponent
  opponent-after-death
  opponent-after-death-winrate
  opponent-death-timing
  death-rank-deficit
  recovery-time
  death-reset-exclusions
  recovery-outcomes
  forfeit-coverage
  forfeit-rating
  forfeit-winrate
  forfeit-residual
  forfeit-pace
  forfeit-completion
  forfeit-improvement
  forfeit-hour-improvement
  forfeit-pace-improvement
  recovery-improvement
  forfeit-death-improvement
  recovery-pace-improvement
  forfeit-retention
  forfeit-cross-season-pace
  forfeit-cross-season-rating
  recovery-elo
  recovery-efficiency
  forfeit-calibration
)
run_scope() {
  local season_scope="$1"
  local scope_dir="$2"
  mkdir -p "$scope_dir"
  for analysis in "${analyses[@]}"; do
    printf 'Running %s for seasons %s\n' "$analysis" "$season_scope"
    direnv exec . target/release/rankewd --dataset dataset --seasons "$season_scope" \
      --analysis "$analysis" --output "$scope_dir/${analysis//-/_}.png" \
      > "$scope_dir/$analysis.txt"
  done
}
run_scope 10,11 "$research_dir"
run_scope all "$research_dir/all_seasons"
mkdir -p "$research_dir/all_seasons/season12_sensitivity"
for analysis in recovery-winrate forfeit-rating forfeit-winrate; do
  printf 'Running %s for partial season 12\n' "$analysis"
  direnv exec . target/release/rankewd --dataset dataset --seasons 12 \
    --analysis "$analysis" --output "$research_dir/all_seasons/season12_sensitivity/${analysis//-/_}.png" \
    > "$research_dir/all_seasons/season12_sensitivity/$analysis.txt"
done
RANKEWD_REPORT_OUTPUT="$research_dir" direnv exec . uv run \
  --with ijson==3.5.1 --with zstandard==0.25.0 --with markdown==3.11 python - <<'PY'
import os
from pathlib import Path
from scripts.build_research_report import build
root = Path(os.environ['RANKEWD_REPORT_OUTPUT'])
build(root / 'all_seasons', seasons=tuple(range(1, 13)))
build(root)
PY
