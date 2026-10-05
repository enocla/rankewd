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
