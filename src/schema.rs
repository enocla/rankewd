//! Typed Rust representation of one match record in the source JSONL export.
//!
//! The runtime analyses consume Parquet projections, while this module remains
//! the public compatibility model for consumers of the original source data.
//! The structs use Serde's camel-case mapping to match the source field names.
//! Source codes that do not have a documented mapping remain integers or
//! strings. Optional fields capture historical omissions and nullable values.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One match record from a season JSON Lines file.
///
/// Each line in a season file deserializes independently as a `MatchRecord`.
/// Historical files may omit `vod` or `gameMode`, and replay availability has
/// appeared under either `replayExist` or the legacy `replayExists` spelling.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRecord {
    /// Numeric match identifier.
    pub id: u64,
    /// Optional source tag; null values are preserved as `None`.
    pub tag: Option<String>,
    /// Optional video-on-demand references. The sampled files do not establish
    /// the shape of items, so they are retained as arbitrary JSON values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vod: Option<Vec<Value>>,
    /// Match start time as Unix seconds.
    pub date: u64,
    /// Seasonal and all-time rank values.
    pub rank: Rank,
    /// Seed metadata.
    pub seed: Seed,
    /// Numeric source match-type code. Its code-to-name mapping is not present
    /// in the dataset.
    #[serde(rename = "type")]
    pub match_type: u64,
    /// Result time in milliseconds and associated player, if any.
    pub result: MatchResult,
    /// Season number.
    pub season: u32,
    /// Per-player rating changes; this list may be empty.
    pub changes: Vec<RatingChange>,
    /// Whether the source marks this match or rating as decayed.
    pub decayed: bool,
    /// Players included in the match record.
    pub players: Vec<Player>,
    /// Whether the source marks this as a beginner match.
    pub beginner: bool,
    /// Source category code, such as `ANY` or `ENTER_NETHER`.
    pub category: String,
    /// Source game-mode label. This field is absent from older season records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_mode: Option<String>,
    /// Seed or starting-structure category, when supplied.
    pub seed_type: Option<String>,
    /// Bot/source label, when supplied. Sampled records contained null.
    pub bot_source: Option<String>,
    /// Whether the match record is marked as forfeited.
    pub forfeited: bool,
    /// Match events. The event type is an open source string, not a closed enum.
    pub timelines: Vec<TimelineEvent>,
    /// Spectator entries. The sampled arrays were empty, so item values remain
    /// arbitrary JSON values.
    pub spectators: Vec<Value>,
    /// Bastion location type, when supplied.
    pub bastion_type: Option<NetherLocation>,
    /// Completed runs associated with the match; this list may be empty.
    pub completions: Vec<Completion>,
    /// Replay-availability flag used by most sampled records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_exist: Option<bool>,
    /// Legacy `replayExists` spelling, observed once with an integer value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_exists: Option<i64>,
}

/// Player identifier stored as 32 hexadecimal characters without hyphens.
///
/// This transparent newtype preserves the source string representation. It
/// does not perform runtime validation of the hexadecimal format.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PlayerId(pub String);

/// Seasonal and all-time rank values.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rank {
    /// Season rank, or null when no rank is recorded.
    pub season: Option<i64>,
    /// All-time rank, or null when no rank is recorded.
    pub all_time: Option<i64>,
}

/// Known Nether bastion location labels found in the sampled records.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetherLocation {
    /// Bridge bastion.
    Bridge,
    /// Housing bastion.
    Housing,
    /// Stables bastion.
    Stables,
    /// Treasure bastion.
    Treasure,
}

/// Seed generation and structure information.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Seed {
    /// Opaque seed identifier, or null when unavailable.
    pub id: Option<String>,
    /// Bastion location for the seed, or null when unavailable.
    pub nether: Option<NetherLocation>,
    /// Four End tower values, or null when tower data is unavailable.
    pub end_towers: Option<[i64; 4]>,
    /// Overworld starting-structure label, or null when unavailable.
    pub overworld: Option<String>,
    /// Opaque seed variation labels.
    pub variations: Vec<String>,
}

/// Match result information.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchResult {
    /// Recorded result or completion time in milliseconds.
    pub time: u64,
    /// Player associated with the result, or null when there is none.
    pub uuid: Option<PlayerId>,
}

/// Rating update for one player.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingChange {
    /// Player whose rating changed.
    pub uuid: PlayerId,
    /// Signed rating change, or null when the source has no value.
    pub change: Option<i64>,
    /// Rating value associated with the change, or null when unavailable.
    pub elo_rate: Option<i64>,
}

/// Player details recorded for a match.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    /// Player identifier.
    pub uuid: PlayerId,
    /// Lowercase two-letter country code as supplied by the source, or null.
    /// The export may contain non-standard country codes.
    pub country: Option<String>,
    /// Player rating rank, or null when unavailable.
    pub elo_rank: Option<i64>,
    /// Player rating, or null when unavailable.
    pub elo_rate: Option<i64>,
    /// Player display name.
    pub nickname: String,
    /// Numeric source role code. Its code-to-name mapping is not in the data.
    pub role_type: u64,
}

/// An event recorded on the match timeline.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    /// Event time in milliseconds on the match clock.
    pub time: u64,
    /// Source event identifier, such as an advancement or timeline event name.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Player associated with this event.
    pub uuid: PlayerId,
    /// Optional event-specific payload whose meaning depends on `event_type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<Value>>,
}

/// A completed run associated with a match.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// Completion time in milliseconds.
    pub time: u64,
    /// Player associated with the completion.
    pub uuid: PlayerId,
}
