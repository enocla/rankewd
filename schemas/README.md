# Dataset schema

The season files in `dataset/` use JSON Lines: each non-empty line is a separate JSON object representing one match. Validate each line against [`match-record.schema.json`](match-record.schema.json); the file as a whole is not a JSON array.

This schema was derived by sampling records at several positions in each of the ten available season files. It describes the common record shape, observed null values, and known field differences between seasons. It is a contract for the checked-in data format, not a claim that the source API has no other values.

## Field reference

| Field | Type | Meaning and notes |
| --- | --- | --- |
| `id` | integer | Numeric match identifier. |
| `tag` | string or null | Optional source tag. |
| `vod` | optional array | Video-on-demand references. The sampled arrays were empty, so item structure is intentionally open. |
| `date` | integer | Match start time as Unix seconds. |
| `rank` | object | `season` and `allTime` rank values; each is an integer or null. |
| `seed` | object | Seed metadata: nullable opaque `id`, nullable Nether area and four tower values, nullable overworld label, and string `variations`. |
| `type` | integer | Source match-type code. Its numeric mapping is not present in the dataset. |
| `result` | object | `time` in milliseconds and nullable result `uuid`. |
| `season` | integer | Season number. |
| `changes` | array of objects | Per-player rating entries with `uuid`, nullable signed `change`, and nullable `eloRate`. May be empty. |
| `decayed` | boolean | Source decay flag. |
| `players` | array of objects | Players with `uuid`, nullable `country`, `eloRank`, and `eloRate`, plus `nickname` and integer `roleType`. |
| `beginner` | boolean | Source beginner-match flag. |
| `category` | string | Source category code, such as `ANY` or `ENTER_NETHER`. |
| `gameMode` | optional string | Source game-mode label. Absent in older season records. |
| `seedType` | string or null | Seed or starting-structure category. |
| `botSource` | string or null | Bot/source label; sampled records had null. |
| `forfeited` | boolean | Source forfeit flag. |
| `timelines` | array of objects | Events with millisecond `time`, string event `type`, player `uuid`, and optional event-specific `data`. |
| `spectators` | array | Spectator entries. Sampled arrays were empty, so item structure is open. |
| `bastionType` | string or null | Observed values are `BRIDGE`, `HOUSING`, `STABLES`, and `TREASURE`. |
| `completions` | array of objects | Completed runs with millisecond `time` and player `uuid`. May be empty. |
| `replayExist` | optional boolean | Replay-availability flag used in most sampled records. |
| `replayExists` | optional integer | Legacy spelling observed once with an integer value. At least one of `replayExist` or `replayExists` is present in each sampled record. |

## Interpreting values

- Player and event UUIDs are represented as 32 hexadecimal characters without hyphens.
- `date` is in Unix seconds. Match, result, completion, and timeline times are in milliseconds.
- Category, game-mode, seed, event, match-type, and role-type values come from the source. The schema leaves most codes open or numeric because the export does not provide their complete enumerations or code mappings.
- Country values are lowercase two-letter strings when present. They are preserved as supplied, including non-standard codes.
- `vod`, `spectators`, and timeline `data` are typed as arrays but their element/payload formats are not inferred from empty or sparse samples.
- Historical records omit some fields. The schema therefore makes `vod` and `gameMode` optional and accepts both replay field spellings.

Objects are closed to undeclared properties so a new field in a future export is visible as a schema change. Update the schema and this reference when such a field is confirmed.
