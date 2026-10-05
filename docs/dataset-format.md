# Match dataset format

Parquet is the runtime format consumed by the CLI and analyses. The original
source export is JSON Lines: each non-empty line is one JSON object for a
match. The source interchange model remains in [`src/schema.rs`](../src/schema.rs)
and is exported as `rankewd::schema`; analyses use small Parquet projections
because they do not need every field in the full source record.

Curated Parquet files contain only rows whose `type` field is `2`. Every
analysis also requires `decayed` and `forfeited` to be `false`; rows with either
value true, or with either value missing, are excluded from all results. The
`games-per-hour` and `hour-entropy` analyses are the exception: they describe
when matches are played rather than how they ended, so they require
`decayed = false` only, keep forfeited matches, and read a missing `forfeited`
value as not forfeited.

## Reading a source record

The crate exposes Serde types, so a consumer can deserialize a JSON Lines
record with `serde_json`:

```rust
use rankewd::schema::MatchRecord;

let record: MatchRecord = serde_json::from_str(json_line)?;
```

The model uses `Option<T>` for nullable values and fields that are absent in older records. Opaque or sparse arrays such as `vod` and `spectators` use `serde_json::Value` so their contents are preserved without guessing a shape.

## Field guide

| JSON field | Rust field | Type | Meaning |
| --- | --- | --- | --- |
| `id` | `id` | `u64` | Numeric match identifier. |
| `tag` | `tag` | `Option<String>` | Optional source tag; null is preserved as `None`. |
| `vod` | `vod` | `Option<Vec<Value>>` | Optional video-on-demand references. Sampled arrays were empty. |
| `date` | `date` | `u64` | Match start time as Unix seconds. |
| `rank` | `rank` | `Rank` | Seasonal and all-time rank values. |
| `seed` | `seed` | `Seed` | Seed identifier, locations, End tower values, and variations. |
| `type` | `match_type` | `u64` | Numeric source match-type code; no mapping is included. |
| `result` | `result` | `MatchResult` | Result time in milliseconds and associated player UUID, if any. |
| `season` | `season` | `u32` | Season number. |
| `changes` | `changes` | `Vec<RatingChange>` | Per-player rating updates; may be empty. |
| `decayed` | `decayed` | `bool` | Source decay flag. |
| `players` | `players` | `Vec<Player>` | Match participants and their rating/profile values. |
| `beginner` | `beginner` | `bool` | Source beginner-match flag. |
| `category` | `category` | `String` | Source category code, such as `ANY` or `ENTER_NETHER`. |
| `gameMode` | `game_mode` | `Option<String>` | Game-mode label; absent in older season records. |
| `seedType` | `seed_type` | `Option<String>` | Seed or starting-structure category. |
| `botSource` | `bot_source` | `Option<String>` | Bot/source label; sampled records had null. |
| `forfeited` | `forfeited` | `bool` | Source forfeit flag. |
| `timelines` | `timelines` | `Vec<TimelineEvent>` | Events with millisecond match-clock time, source type string, player UUID, and optional payload. |
| `spectators` | `spectators` | `Vec<Value>` | Spectator entries; sampled arrays were empty. |
| `bastionType` | `bastion_type` | `Option<NetherLocation>` | Bastion location, when supplied. |
| `completions` | `completions` | `Vec<Completion>` | Completed runs; may be empty. |
| `replayExist` | `replay_exist` | `Option<bool>` | Replay-availability flag used in most sampled records. |
| `replayExists` | `replay_exists` | `Option<i64>` | Legacy spelling, observed once with an integer value. |

## Source value notes

- Player and event UUIDs use 32 hexadecimal characters without hyphens. `PlayerId` preserves that string; it does not validate its format.
- Match, result, completion, and timeline times are milliseconds. The `date` field is Unix seconds.
- `NetherLocation` represents the observed `BRIDGE`, `HOUSING`, `STABLES`, and `TREASURE` labels. The category, seed, game-mode, match-type, event-type, and player-role values otherwise remain open strings or source integers because the export does not provide complete mappings.
- Country values are lowercase two-letter strings when present, but are preserved as supplied and may include non-standard codes.
- The model was based on records sampled at several positions in the available season files. Update the types if future exports introduce confirmed fields or value variants.

## Rolling win duration

Run `rankewd --analysis rolling-win-duration` to plot one line for every
player with more than 100 games in each selected season. Events are ordered by
match date and ID, and each plotted point is the arithmetic mean duration, in
seconds, of that player's ten most recent qualifying wins. A qualifying win has
`type = 2`, `decayed = false`, and `forfeited = false`, and its winner UUID
appears in `players`. Its duration comes from `result.time`, which is expressed
in milliseconds. The horizontal axis uses the first and last match dates of the
season, and lower durations indicate improvement. A green line overlays the
equal-weight mean player duration in fixed season-progress bins: the mean of each
player's durations in a bin, averaged over the players that reach that bin. The
reported improvement compares the mean of the first ten retained bins with the
mean of the last ten, after dropping every bin below 10% season progress.

## Hour-of-day distribution

Run `rankewd --analysis games-per-hour` to count matches by the UTC hour their
`date` falls in, one bin per hour from `00:00` to `23:00` UTC, and
`rankewd --analysis hour-entropy` to bin the same counts per player. Both keep
forfeited matches and drop only decayed rows, because a forfeit still happened
at a particular hour, and each per-season player is credited with one match in
an hour per match they appear in.

`games_per_hour_season_N.png` draws one bar per UTC hour and highlights the
busiest hour in yellow, with that hour also named in the chart title. The
report repeats the busiest and quietest hours with their match counts and
shares, and the share of matches that were forfeited.

`hour_entropy_season_N.png` draws one bar per 0.25-bit entropy bin and marks the
median with a dashed line. A player's entropy is the Shannon entropy, in bits,
of their own 24-hour distribution: 0 bits means every one of their matches fell
in a single UTC hour, and `log2(24) = 4.58` bits means an even spread across the
whole day. A bin value therefore also reads as `2^bits`, the number of hours a
player would have to use equally to reach the same spread. Players with fewer
than 100 matches are skipped and counted in the report, because a plug-in
entropy over a handful of games mostly measures how few games there were. Both
analyses group the counts by the `season` column, so `--seasons all` writes one
chart per season.

## Player-season summary data

Run `rankewd --analysis elo-versus-games` to aggregate match files by season and player UUID. By default, the CLI uses `dataset/seasons_10_11.parquet` but filters it to season 11; use `--dataset dataset --seasons all` to select every `season_N_*.parquet` file. Elo and games played are kept separate for each season, since seasonal ratings reset. Win counting treats `result.uuid` as the winner and counts a match as decided only when that UUID appears in the participant list. The analysis writes `output/player_elos.parquet` with one row per player per season and these columns:

| Column | Type | Meaning |
| --- | --- | --- |
| `season` | `uint32` | Season identifier. |
| `uuid` | `string` | Player identifier. |
| `nickname` | `string` | Most recently observed nickname in that season. |
| `games_played` | `int64` | Match appearances in that season. |
| `wins` | `int64` | Matches won, based on `result.uuid`. |
| `decided_matches` | `int64` | Matches with a recorded winner included in the participant list. |
| `win_rate` | nullable `float64` | `wins / decided_matches`; null when there are no decided matches. |
| `latest_elo` | nullable `int64` | Latest non-null Elo recorded for that player in that season. |
| `latest_elo_date` | nullable `int64` | Unix seconds timestamp for that Elo observation. |

The analysis writes `elo_vs_games_season_N.png`, `elo_distribution_season_N.png`, `elo_vs_win_duration_season_N.png`, and `name_underscores_vs_elo_season_N.png` for each season containing a rated player. The Elo distribution chart uses 50 point bins. `elo_vs_win_duration_season_N.png` scatters rated players by their final Elo and the arithmetic mean, in seconds, of `result.time` over their qualifying wins, using a logarithmic duration axis. A qualifying win has `type = 2`, `decayed = false`, and `forfeited = false`, its winner UUID appears in `players`, and its `result.time` is greater than zero. That chart requires at least 10 qualifying wins per player, because a mean over fewer wins is too noisy to correlate, and it reports how many rated players were excluded. `name_underscores_vs_elo_season_N.png` draws one box per underscore count in the nickname recorded alongside each player's final Elo. Boxes span the first to third quartile with the median in red, and whiskers reach the furthest value still within 1.5xIQR of the quartiles; values past the fences are counted as outliers and reported per group. Groups below 20 players are skipped and reported, because a box of a handful of players would not describe a distribution. The two win-rate charts are written for seasons with qualifying data and are limited to player-season rows with at least 100 match appearances. `winrate_vs_elo_season_N.png` shows player rates and a mean player rate line for 100 Elo bins with at least 20 players. `winrate_over_time_season_N.png` plots each qualifying player's match-time Elo against their cumulative win rate through that match; a smooth color gradient shows progress from the first to last match date in the season. The Parquet table retains all players regardless of these chart filters. Matches without a recorded winner are excluded from win-rate denominators and do not update cumulative rates. When `--output PATH` is supplied, its parent becomes the output directory, its stem is used for the primary Elo-versus-games chart, and the other artifacts use their standard names. Existing files are not removed when a season no longer qualifies.


## Playerbase plots

The twenty playerbase analyses listed in the README read one shared
projection of each match. Only rows with `decayed = false` and
`forfeited = false` and exactly two distinct participant UUIDs contribute.

- **Elo** is `changes[].eloRate`, the rating a player carried into the match;
  the rating after it is `eloRate + change`. `players[].eloRate` is a profile
  snapshot that stays constant across many matches and is not used. A player's
  final Elo is their last post-match rating of the season and their peak the
  highest one. Season 1 stores `changes[].eloRate` as a double; it is rounded.
- **Elo expectation** is `1 / (1 + 10^((opponent - own) / 400))`. Charts that
  report win rate "vs Elo expectation" average the outcome minus this value.
- **Milestones** are the first `timelines` event of each type per player:
  `story.enter_the_nether`, `nether.find_bastion`, `nether.find_fortress`,
  `projectelo.timeline.blind_travel`, `story.follow_ender_eye`, and
  `story.enter_the_end`. Deaths count `projectelo.timeline.death` events.
  `split-evolution` keeps only wins that record all six milestones.
- **Seed and bastion type** come from `seedType` and `bastionType`, falling back
  to `seed.overworld` and `seed.nether` when a season leaves them empty.
- **Sessions** end when a player's next match starts more than 30 minutes after
  the previous one ended (`date + result.time`).
- **Veterans** are players who appear in an earlier selected season, so
  `veteran-edge` and `season-retention` need several seasons selected.
- **Countries** use the last country code a player-season lists; player-seasons
  without one are counted in the report and skipped.


## Converting source JSONL exports

The reusable importer is [`scripts/jsonl_to_parquet.py`](../scripts/jsonl_to_parquet.py).
It infers the schema from the complete export, writes Zstandard-compressed
Parquet, and keeps only `type = 2`. It preserves real forfeits and synthetic
decay records; each analysis applies its own documented filter. Historical
fractional `changes[].eloRate` values remain doubles. Rating changes are stored
as signed 64-bit integers.

Run from the repository root through the Nix shell. `uv` supplies DuckDB for the
import session; no Python runtime dependency is added to the Rust application:

```sh
direnv exec . uv run --with duckdb==1.5.6 python - <<'PY'
from pathlib import Path
from scripts.jsonl_to_parquet import convert
for source in sorted(Path('.').glob('season_[34]_*.jsonl')):
    convert(source, Path('dataset') / source.with_suffix('.parquet').name)
PY
```

An explicit destination places curated input in `dataset/`. Calling
`convert(source)` without one writes to `output/curated/`. Existing files are
never overwritten. Each import verifies the row count, unique match-ID count,
season labels, absence of other match types, and a checksum across research
fields before publishing the file. Raw JSONL files remain unchanged.

## Forfeit and recovery research

The `opponent-after-death`, `opponent-after-death-winrate`, `opponent-death-timing`, `recovery-*`, `death-reset-exclusions`, `death-rank-deficit`, and `forfeit-*`
analyses are an explicit exception requested for this study: they keep real
forfeited matches while excluding decay. Other analyses retain their existing
filters. Match-level `forfeited` means no completions; individual quit exposure
comes from `projectelo.timeline.forfeit`. Unknown quit outcomes are not silently
assigned to a player.

The shared loader uses the first ordinary death as the sole recovery exposure.
World-reset events are ignored completely: they do not trigger exposures, affect
continuation status, contribute to opponent adversity, restart the recorded split
history, or set the fallback event bound for missing clocks. Intentional
spawnpoint deaths (`projectelo.timeline.death_spawnpoint`) remain excluded from
all research exposures and retained only as coverage-audit counts.

Every recovery and opponent-outcome plot uses deaths with **no recorded own
forfeit in that match**, without requiring another split. This retains players
who lose before another split. Absence of an explicit forfeit is an observable
proxy, not proof of active play; unclassified exits remain eligible. Observation
stops at the earliest completion, either player's first forfeit, or match-end
bound. Events after that cutoff cannot contribute. The ordinary-death count
after a recorded forfeit is audited. The win-rate CSV retains all-first-death
outcomes as a separate descriptive reference (behavior 0); behavior 1 is the
no-own-forfeit cohort plotted. Recovery confidence intervals conservatively
cluster both players and matches.

Split context is cumulative recorded progress before the death, because world
reset events are ignored; it is not necessarily position in the current attempt.
The Elo-efficiency analyses start at first ordinary death and retain forfeiting
as the comparison action. Habit profiles retain all rated matches to measure
forfeit rates. Neither analysis uses reset events. Longitudinal continuation
exposure is the fraction of ordinary-death matches with no own forfeit, without
requiring a later split; at least ten prior death matches are required.
Habit and improvement intervals cluster repeated UUIDs. These comparisons
are observational rather than randomized continuation policies.

Each research plot writes a PNG and matching CSV. A source-fingerprinted Zstandard-compressed JSON
cache avoids rescanning all match timelines for every plot; it includes source
paths, sizes, modification times, requested seasons and measurement version.
`--output` relocates the chart, CSV and shared cache to the selected family.
Neither the merged season 10–11 copy nor duplicate match IDs are double-counted.

Performance profiles require 100 rated games. Learning cohorts qualify from
50 previous rated games at season days 30, 60 and 90, are active within the
previous 14 days, and have a complete next-30-day export window. Outcome
estimates require at least 10 later rated games; zero-follow-up cohorts are
reported. Completion-pace comparisons also require at least 10 completions
in each block and shared seed/bastion cells with two completions per block.
Only cells sharing at least 10 weighted completions are compared.

Longitudinal outcomes are centered within season, landmark, baseline rank,
200-Elo band, five-minute completion-pace band, and 200-Elo prior-gain band.
The continuous exposure slope demeans both exposure and outcome in those
strata. Returning-player comparisons use adjacent seasons and previous last
50 / subsequent first 50 rated games. Rating position compares percentiles
within those corresponding windows, not raw Elo across resets. These controls
reduce measured confounding but do not identify causal effects.

Efficiency compares explicit quit outcomes after death with no-own-forfeit outcomes, within
season/100-Elo band/opponent-gap/split/deficit cells represented by at least
20 observations in each action group. Both groups use identical cell weights.
Ten-game horizons include the current outcome and nine subsequent rated games;
time is the current remaining match clock plus subsequent match durations.
The session sensitivity additionally requires gaps no longer than 30 minutes.
Queue time, penalties, unobserved practice and horizon-selection bias remain
outside the estimate. Ratios use aggregate Elo divided by aggregate time.


Short cross-season completion blocks additionally use a broader seed-type
matching estimate: combine bastion layouts within each overworld seed type,
require two completions in a shared type and ten weighted common completions.
The within-season and recovery-pace plots retain the stricter seed-and-layout
matching. This difference is explicit because exact layout matching leaves too
few cross-season pairs for reliable rank-level curves.

Season 1 contains no ordinary-death events; its
normal-death probability is unavailable, not zero. Early-season Elo bands use
the same numerical thresholds for comparison and need not represent historical
rank names (season 1 had a different rank system).

The baseline-stratum plots and slopes exclude strata with no exposure variation
(including singletons). Their CLI reports distinguish initial measurable cohorts
from observations with comparable peers. Completion-pace evidence remains
conditional on enough observed completions; it cannot rule out learning on
uncompleted attempts. Cache publication is atomic and compressed.

Older draws sometimes have `result.time = 0`. They remain in outcome and
habit denominators. A last-event bound validates their recorded event ordering
but is never counted as observed playing time. Elo/hour uses only outcomes
with measured durations, and ten-game efficiency requires measured clocks for
every game in the horizon. The audit reports missing-duration counts.

`bash scripts/run-forfeit-research.sh` reproduces 30 main plots restricted to
seasons 10 and 11, a separate all-season recovery comparison, and the
Markdown/HTML illustrated report. The report
builder uses pinned Python formatting dependencies through `uv` and embeds a
local copy of SF Pro Text. The main report checks that its measurement cache
contains only seasons 10 and 11. Cross-season main analyses therefore cover
only the 10-to-11 transition. The earlier complete all-season report is
preserved separately in `output/research/all_seasons/`. It formats Rust
estimates; statistical estimation remains in the registered Rust analyses.


Opponent-after-death comparisons count each player-match at the first ordinary
player death. Intentional spawnpoint deaths are excluded. An opponent event
must occur strictly later and no later than the match end; ties are not later.
The last ordinary opponent death detects repeat deaths even if the opponent had
already died before the player. Explicit opponent forfeits are distinct from
unclassified exits. Death and forfeit frequencies overlap; the win-rate plot
uses mutually exclusive neither/death-only/forfeit-only/both groups. A second
plot separates earlier and later ordinary opponent deaths. All first deaths and
the selected continuation subset are reported separately. Own forfeits can end
observation before an opponent event occurs. The comparisons describe observed
future events, not a probability forecast available at the death, a fixed-time
hazard or a causal effect. Intervals use conservative player/match clustering.
