# Season dataset format

The files in `dataset/` are JSON Lines: each non-empty line is one JSON object for a match, rather than one JSON array containing all matches. The Rust data model is in [`src/schema.rs`](../src/schema.rs) and is exported as `rankewd::schema`.

## Convert all seasons to Parquet

Run `cargo run --release --bin dataset_to_parquet`. It reads every `*.jsonl` file directly inside `dataset/`, writes one Parquet file per season into `dataset/parquet/`, and reports Polars row counts for each output and the total. Optional positional arguments override the input and output directories:

```sh
cargo run --release --bin dataset_to_parquet -- dataset /path/to/parquet
```

The converter uses Polars' lazy JSON Lines reader and Parquet sink to process each season without collecting the full dataset into memory. It infers each file's schema from all rows so sparse fields are included.

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
