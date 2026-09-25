# Season dataset format

The files in `dataset/` are JSON Lines: each non-empty line is one JSON object for a match, rather than one JSON array containing all matches. The Rust data model is in [`src/schema.rs`](../src/schema.rs) and is exported as `rankewd::schema`.

## Reading a record

The crate exposes Serde types, so a consumer can deserialize each line with `serde_json`:

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
- The model was based on records sampled at several positions in each of the ten available season files. Update the types if future exports introduce confirmed fields or value variants.

## Player-season summary data

Run `rankewd --analysis elo-versus-games` to aggregate match files by season and player UUID. By default, this analysis reads all files named `season_N_*.parquet` in `dataset/`; `--seasons` can limit the selection. Elo and games played are kept separate for each season, since seasonal ratings reset. Win counting treats `result.uuid` as the winner and counts a match as decided only when that UUID appears in the participant list. The analysis writes `dataset/player_elos.parquet` with one row per player per season and these columns:

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

The analysis writes `elo_vs_games_season_N.png` and `elo_distribution_season_N.png` for each season containing a rated player. The Elo distribution chart uses 50 point bins. The two win-rate charts are written for seasons with qualifying data and are limited to player-season rows with at least 100 match appearances. `winrate_vs_elo_season_N.png` shows player rates and a mean player rate line for 100 Elo bins with at least 20 players. `winrate_over_time_season_N.png` plots each qualifying player's match-time Elo against their cumulative win rate through that match; a smooth color gradient shows progress from the first to last match date in the season. The Parquet table retains all players regardless of these chart filters. Matches without a recorded winner are excluded from win-rate denominators and do not update cumulative rates. When `--output PATH` is supplied, its filename stem is used for the Elo-versus-games charts, with the season appended; the other charts use their standard names in the same directory.
