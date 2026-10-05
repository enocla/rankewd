use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::file::statistics::Statistics;
use parquet::record::{Field, List, Row};
use parquet::schema::parser::parse_message_type;
use parquet::schema::types::{Type, TypePtr};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;

pub(crate) type ParquetFile = SerializedFileReader<File>;

/// Lowest match count before a player's win rate is stable enough to chart.
pub(crate) const MIN_GAMES_FOR_WINRATE_PLOTS: u64 = 100;

/// Hours in one day, the resolution of the hour-of-day analyses.
pub(crate) const HOURS_PER_DAY: usize = 24;

/// Cumulative win-rate snapshots kept for every sampled player.
pub(crate) const WINRATE_POINTS_PER_PLAYER: usize = 5;

const WINRATE_SAMPLE_SIZE: usize = 100;
const WINRATE_SAMPLE_SEED: u64 = 0x5241_4e4b_4557_4432;
const PLAYER_PROJECTION: &str = "message root { \
    optional int64 id; \
    optional int64 date; \
    optional int64 season; \
    optional int64 type; \
    optional boolean decayed; \
    optional boolean forfeited; \
    optional group result { \
        optional int64 time; \
        optional binary uuid (STRING); \
    } \
    optional group players (LIST) { \
        repeated group list { \
            optional group element { \
                optional binary uuid (STRING); \
                optional int64 eloRate; \
                optional binary nickname (STRING); \
            } \
        } \
    } \
}";
const RECORD_FILTER_PROJECTION: &str = "message root { \
    optional int64 season; \
    optional boolean decayed; \
    optional boolean forfeited; \
}";
const PLAYER_SUMMARY_PROJECTION: &str = "message root { \
    optional int64 date; \
    optional int64 season; \
    optional boolean decayed; \
    optional boolean forfeited; \
    optional group result { \
        optional binary uuid (STRING); \
    } \
    optional group players (LIST) { \
        repeated group list { \
            optional group element { \
                optional binary uuid (STRING); \
                optional int64 eloRate; \
                optional binary nickname (STRING); \
            } \
        } \
    } \
}";
const HOUR_OF_DAY_PROJECTION: &str = "message root { \
    optional int64 date; \
    optional int64 season; \
    optional boolean decayed; \
    optional boolean forfeited; \
    optional group players (LIST) { \
        repeated group list { \
            optional group element { \
                optional binary uuid (STRING); \
            } \
        } \
    } \
}";

const SECONDS_PER_HOUR: i64 = 3_600;
const SECONDS_PER_DAY: i64 = 86_400;

pub(crate) fn open(path: &Path) -> Result<ParquetFile, Box<dyn Error>> {
    Ok(SerializedFileReader::new(File::open(path)?)?)
}

pub(crate) fn record_count(path: &Path, seasons: Option<&[u32]>) -> Result<i64, Box<dyn Error>> {
    let requested = seasons.map(|seasons| seasons.iter().copied().collect::<BTreeSet<_>>());
    if requested.as_ref().is_some_and(BTreeSet::is_empty) {
        return Ok(0);
    }

    let reader = open(path)?;
    let projection = compatible_projection(&reader, RECORD_FILTER_PROJECTION)?;
    let mut count = 0i64;
    for row in reader.get_row_iter(Some(projection))? {
        let row = row?;
        if !passes_default_filters(&row)? {
            continue;
        }
        let season = integer_field(&row, "season")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing season column"))?;
        let season = u32::try_from(season).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
        })?;
        if requested
            .as_ref()
            .is_none_or(|requested| requested.contains(&season))
        {
            count = count
                .checked_add(1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "row count overflow"))?;
        }
    }
    Ok(count)
}

pub(crate) fn integer_field(row: &Row, name: &str) -> io::Result<Option<i64>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Long(value)) => Ok(Some(*value)),
        Some(Field::Int(value)) => Ok(Some(i64::from(*value))),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value)),
        None => Err(missing_field(name)),
    }
}

pub(crate) fn boolean_field(row: &Row, name: &str) -> io::Result<Option<bool>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Bool(value)) => Ok(Some(*value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value)),
        None => Err(missing_field(name)),
    }
}

/// Applies the match filters every analysis uses by default.
pub(crate) fn passes_default_filters(row: &Row) -> io::Result<bool> {
    Ok(boolean_field(row, "decayed")? == Some(false)
        && boolean_field(row, "forfeited")? == Some(false))
}

pub(crate) fn string_field<'a>(row: &'a Row, name: &str) -> io::Result<Option<&'a str>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Str(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value)),
        None => Err(missing_field(name)),
    }
}

pub(crate) fn list_field<'a>(row: &'a Row, name: &str) -> io::Result<Option<&'a List>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::ListInternal(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value)),
        None => Err(missing_field(name)),
    }
}

pub(crate) fn group_field<'a>(row: &'a Row, name: &str) -> io::Result<Option<&'a Row>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Group(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value)),
        None => Err(missing_field(name)),
    }
}

fn find_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a Field>, io::Error> {
    Ok(row
        .get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value)))
}

fn missing_field(name: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("missing {name} column"))
}

pub(crate) fn invalid_field(name: &str, value: &Field) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unexpected value for {name}: {value:?}"),
    )
}

/// Player rows and per-season match history read from the selected datasets.
pub(crate) struct PlayerSeasons {
    players: Vec<PlayerSeason>,
    history_by_season: BTreeMap<u32, SeasonHistory>,
}

#[derive(Default)]
struct PlayerSeasonsChunk {
    players: BTreeMap<(u32, String), PlayerSeason>,
    history_by_season: BTreeMap<u32, SeasonHistory>,
}

/// Everything one season's plots need: its players and their match history.
pub(crate) struct SeasonData<'a> {
    pub(crate) season: u32,
    pub(crate) players: Vec<&'a PlayerSeason>,
    pub(crate) history: &'a SeasonHistory,
}

/// One player's season totals and most recent Elo rating.
pub(crate) struct PlayerSeason {
    pub(crate) season: u32,
    pub(crate) uuid: String,
    pub(crate) games_played: u64,
    pub(crate) latest_elo: Option<i64>,
    pub(crate) latest_underscore_count: Option<usize>,
    wins: u64,
    decided_matches: u64,
    latest_elo_date: Option<i64>,
}

/// One player's participation in a single match.
struct Participant {
    uuid: String,
    elo: Option<i64>,
    underscore_count: Option<usize>,
}

#[derive(Default)]
pub(crate) struct SeasonHistory {
    players: HashMap<String, Vec<TemporalEvent>>,
    first_date: Option<i64>,
    last_date: Option<i64>,
}

#[derive(Clone, Copy)]
pub(crate) struct TemporalEvent {
    pub(crate) date: i64,
    pub(crate) match_id: i64,
    elo: Option<i64>,
    pub(crate) won: bool,
    decided: bool,
    pub(crate) match_type: Option<i64>,
    pub(crate) result_time_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TemporalPoint {
    pub(crate) elo: i64,
    pub(crate) win_rate: f64,
    pub(crate) progress: f64,
}

#[derive(Default)]
pub(crate) struct SampledTemporalPoints {
    pub(crate) player_count: usize,
    pub(crate) points: Vec<TemporalPoint>,
}

#[derive(Clone, Copy)]
struct TemporalCandidate {
    date: i64,
    elo: i64,
    win_rate: f64,
}

impl PlayerSeason {
    pub(crate) fn win_rate(&self) -> Option<f64> {
        (self.decided_matches > 0).then(|| self.wins as f64 / self.decided_matches as f64)
    }
}

/// Statistics are optional; a missing or unrecognized value must be read normally.
pub(crate) fn row_group_matches(
    reader: &ParquetFile,
    index: usize,
    seasons: Option<&[u32]>,
) -> bool {
    let Some(seasons) = seasons else {
        return true;
    };
    let Some(column) = reader
        .metadata()
        .row_group(index)
        .columns()
        .iter()
        .find(|column| {
            column.column_path().parts().len() == 1 && column.column_path().parts()[0] == "season"
        })
    else {
        return true;
    };
    let bounds = match column.statistics() {
        Some(Statistics::Int64(stats)) => {
            stats.min_opt().zip(stats.max_opt()).map(|(a, b)| (*a, *b))
        }
        Some(Statistics::Int32(stats)) => stats
            .min_opt()
            .zip(stats.max_opt())
            .map(|(a, b)| (i64::from(*a), i64::from(*b))),
        _ => None,
    };
    bounds.is_none_or(|(min, max)| {
        seasons
            .iter()
            .any(|season| (min..=max).contains(&i64::from(*season)))
    })
}

impl PlayerSeasonsChunk {
    fn add_row(
        &mut self,
        row: &Row,
        seasons: Option<&[u32]>,
        keep_history: bool,
    ) -> io::Result<()> {
        if !passes_default_filters(row)? {
            return Ok(());
        }

        let season = integer_field(row, "season")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing season"))?;
        let season = u32::try_from(season).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
        })?;
        if seasons.is_some_and(|requested| !requested.contains(&season)) {
            return Ok(());
        }

        let date = integer_field(row, "date")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing match date"))?;
        let match_id = if keep_history {
            integer_field(row, "id")?.unwrap_or_default()
        } else {
            0
        };
        let match_type = if keep_history {
            integer_field(row, "type")?
        } else {
            None
        };
        let season_history = self.history_by_season.entry(season).or_default();
        season_history.first_date = Some(
            season_history
                .first_date
                .map_or(date, |first| first.min(date)),
        );
        season_history.last_date =
            Some(season_history.last_date.map_or(date, |last| last.max(date)));

        let Some(participants) = list_field(row, "players")? else {
            return Ok(());
        };
        let result = group_field(row, "result")?;
        let winner_uuid = result
            .map(|result| string_field(result, "uuid"))
            .transpose()?
            .flatten();
        let result_time_ms = if keep_history {
            result
                .map(|result| integer_field(result, "time"))
                .transpose()?
                .flatten()
        } else {
            None
        };
        let mut match_participants = Vec::new();
        let mut seen_in_match = HashSet::new();

        for participant in participants.elements() {
            let Field::Group(participant) = participant else {
                return Err(invalid_field("player entry", participant));
            };
            let Some(uuid) = string_field(participant, "uuid")? else {
                continue;
            };
            if !seen_in_match.insert(uuid.to_owned()) {
                continue;
            }

            match_participants.push(Participant {
                uuid: uuid.to_owned(),
                elo: integer_field(participant, "eloRate")?,
                underscore_count: string_field(participant, "nickname")?
                    .map(|name| name.chars().filter(|character| *character == '_').count()),
            });
        }
        let has_winner = winner_uuid.as_ref().is_some_and(|winner| {
            match_participants
                .iter()
                .any(|participant| &participant.uuid == winner)
        });

        for participant in match_participants {
            let won = winner_uuid
                .as_ref()
                .is_some_and(|winner| winner == &participant.uuid);
            let player = self
                .players
                .entry((season, participant.uuid.clone()))
                .or_insert_with(|| PlayerSeason {
                    season,
                    uuid: participant.uuid.clone(),
                    games_played: 0,
                    wins: 0,
                    decided_matches: 0,
                    latest_elo: None,
                    latest_underscore_count: None,
                    latest_elo_date: None,
                });
            player.games_played = player.games_played.checked_add(1).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "games played overflow")
            })?;
            if has_winner {
                player.decided_matches =
                    player.decided_matches.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "decided match count overflow")
                    })?;
                if won {
                    player.wins = player.wins.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "win count overflow")
                    })?;
                }
            }
            if let Some(elo) = participant.elo
                && player
                    .latest_elo_date
                    .is_none_or(|latest_date| date >= latest_date)
            {
                player.latest_elo = Some(elo);
                player.latest_elo_date = Some(date);
                player.latest_underscore_count = participant.underscore_count;
            }
            if keep_history {
                season_history
                    .players
                    .entry(participant.uuid)
                    .or_default()
                    .push(TemporalEvent {
                        date,
                        match_id,
                        elo: participant.elo,
                        won,
                        decided: has_winner,
                        match_type,
                        result_time_ms,
                    });
            }
        }
        Ok(())
    }

    fn merge(&mut self, chunk: Self) -> io::Result<()> {
        for (key, incoming) in chunk.players {
            if let Some(player) = self.players.get_mut(&key) {
                player.games_played = player
                    .games_played
                    .checked_add(incoming.games_played)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "games played overflow")
                    })?;
                player.decided_matches = player
                    .decided_matches
                    .checked_add(incoming.decided_matches)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "decided match count overflow")
                    })?;
                player.wins = player.wins.checked_add(incoming.wins).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "win count overflow")
                })?;
                if let Some(date) = incoming.latest_elo_date
                    && player
                        .latest_elo_date
                        .is_none_or(|latest_date| date >= latest_date)
                {
                    player.latest_elo = incoming.latest_elo;
                    player.latest_elo_date = Some(date);
                    player.latest_underscore_count = incoming.latest_underscore_count;
                }
            } else {
                self.players.insert(key, incoming);
            }
        }
        for (season, history) in chunk.history_by_season {
            let merged = self.history_by_season.entry(season).or_default();
            if let Some(date) = history.first_date {
                merged.first_date = Some(merged.first_date.map_or(date, |first| first.min(date)));
            }
            if let Some(date) = history.last_date {
                merged.last_date = Some(merged.last_date.map_or(date, |last| last.max(date)));
            }
            for (uuid, mut events) in history.players {
                merged.players.entry(uuid).or_default().append(&mut events);
            }
        }
        Ok(())
    }
}

impl PlayerSeasons {
    /// Reads player totals and match history for every requested season.
    pub(crate) fn load<'a>(
        paths: impl IntoIterator<Item = &'a Path>,
        seasons: Option<&[u32]>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_inner(paths, seasons, true)
    }

    /// Reads only season totals for plots that do not use match history.
    pub(crate) fn load_summaries<'a>(
        paths: impl IntoIterator<Item = &'a Path>,
        seasons: Option<&[u32]>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_inner(paths, seasons, false)
    }

    fn load_inner<'a>(
        paths: impl IntoIterator<Item = &'a Path>,
        seasons: Option<&[u32]>,
        keep_history: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let mut combined = PlayerSeasonsChunk::default();
        for path in paths {
            let reader = open(path)?;
            let projection = compatible_projection(
                &reader,
                if keep_history {
                    PLAYER_PROJECTION
                } else {
                    PLAYER_SUMMARY_PROJECTION
                },
            )?;
            let chunks: Result<Vec<_>, io::Error> = (0..reader.num_row_groups())
                .into_par_iter()
                .filter(|&index| row_group_matches(&reader, index, seasons))
                .map(|index| {
                    // File-backed Parquet readers share a seek cursor across clones.
                    // Give each worker its own file descriptor for concurrent reads.
                    let worker_reader =
                        open(path).map_err(|error| io::Error::other(error.to_string()))?;
                    let row_group = worker_reader
                        .get_row_group(index)
                        .map_err(io::Error::other)?;
                    let mut chunk = PlayerSeasonsChunk::default();
                    for row in row_group
                        .get_row_iter(Some(projection.clone()))
                        .map_err(io::Error::other)?
                    {
                        chunk.add_row(&row.map_err(io::Error::other)?, seasons, keep_history)?;
                    }
                    Ok(chunk)
                })
                .collect();
            for chunk in chunks? {
                combined.merge(chunk)?;
            }
        }

        if combined.players.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no player records found in selected files",
            )
            .into());
        }

        Ok(Self {
            players: combined.players.into_values().collect(),
            history_by_season: combined.history_by_season,
        })
    }

    /// Groups every player by season and pairs each group with its history.
    pub(crate) fn all_player_seasons(&self) -> Vec<SeasonData<'_>> {
        let mut by_season = BTreeMap::<u32, Vec<&PlayerSeason>>::new();
        for player in &self.players {
            by_season.entry(player.season).or_default().push(player);
        }

        by_season
            .into_iter()
            .map(|(season, players)| SeasonData {
                season,
                players,
                history: &self.history_by_season[&season],
            })
            .collect()
    }

    /// Groups rated players by season and pairs each group with its history.
    pub(crate) fn seasons(&self) -> Result<Vec<SeasonData<'_>>, Box<dyn Error>> {
        let mut by_season = BTreeMap::<u32, Vec<&PlayerSeason>>::new();
        for player in &self.players {
            if player.latest_elo.is_some() {
                by_season.entry(player.season).or_default().push(player);
            }
        }
        if by_season.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no players had an Elo value in the selected files",
            )
            .into());
        }

        Ok(by_season
            .into_iter()
            .map(|(season, players)| SeasonData {
                season,
                players,
                history: &self.history_by_season[&season],
            })
            .collect())
    }
}

impl SeasonHistory {
    /// Season start, season end, and length in seconds.
    pub(crate) fn timeline(&self) -> Result<(i64, i64, f64), Box<dyn Error>> {
        let first_date = self.first_date.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "season history has no dates")
        })?;
        let last_date = self.last_date.unwrap_or(first_date);
        let duration = last_date.saturating_sub(first_date).max(1) as f64;
        Ok((first_date, last_date, duration))
    }

    /// One player's match events in dataset load order.
    pub(crate) fn player_events(&self, uuid: &str) -> Option<&[TemporalEvent]> {
        self.players.get(uuid).map(Vec::as_slice)
    }

    /// Cumulative win rate at every rated, decided match of the given players.
    pub(crate) fn points(
        &self,
        eligible_uuids: &HashSet<String>,
    ) -> Result<Vec<TemporalPoint>, Box<dyn Error>> {
        let (first_date, _, duration) = self.timeline()?;
        let mut points = Vec::new();

        for (uuid, events) in &self.players {
            if !eligible_uuids.contains(uuid.as_str()) {
                continue;
            }
            points.extend(
                temporal_candidates(events)
                    .into_iter()
                    .map(|candidate| TemporalPoint {
                        elo: candidate.elo,
                        win_rate: candidate.win_rate,
                        progress: progress_of(candidate.date, first_date, duration),
                    }),
            );
        }

        Ok(points)
    }

    /// Evenly spaced cumulative win-rate snapshots for a reproducible sample of
    /// the given players.
    pub(crate) fn sample_points(
        &self,
        eligible_uuids: &HashSet<String>,
        season: u32,
    ) -> Result<SampledTemporalPoints, Box<dyn Error>> {
        let (first_date, last_date, duration) = self.timeline()?;
        let mut eligible: Vec<_> = self
            .players
            .iter()
            .filter(|(uuid, events)| {
                eligible_uuids.contains(uuid.as_str())
                    && events
                        .iter()
                        .filter(|event| event.decided && event.elo.is_some())
                        .count()
                        >= WINRATE_POINTS_PER_PLAYER
            })
            .map(|(uuid, _)| uuid.clone())
            .collect();
        eligible.sort_unstable();

        let mut rng = StdRng::seed_from_u64(WINRATE_SAMPLE_SEED ^ u64::from(season));
        eligible.shuffle(&mut rng);
        eligible.truncate(WINRATE_SAMPLE_SIZE);

        let mut sampled = SampledTemporalPoints {
            player_count: eligible.len(),
            points: Vec::with_capacity(eligible.len().saturating_mul(WINRATE_POINTS_PER_PLAYER)),
        };
        for uuid in eligible {
            let events = self.players.get(&uuid).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing player history")
            })?;
            let candidates = temporal_candidates(events);
            for candidate in select_evenly_spaced(&candidates, first_date, last_date) {
                sampled.points.push(TemporalPoint {
                    elo: candidate.elo,
                    win_rate: candidate.win_rate,
                    progress: progress_of(candidate.date, first_date, duration),
                });
            }
        }

        Ok(sampled)
    }
}

fn progress_of(date: i64, first_date: i64, duration: f64) -> f64 {
    ((date.saturating_sub(first_date) as f64) / duration).clamp(0.0, 1.0)
}

fn temporal_candidates(events: &[TemporalEvent]) -> Vec<TemporalCandidate> {
    let mut events = events.to_vec();
    events.sort_unstable_by_key(|event| (event.date, event.match_id));
    let mut candidates = Vec::new();
    let mut wins = 0u64;
    let mut decided_matches = 0u64;
    for event in events {
        if event.decided {
            decided_matches += 1;
            if event.won {
                wins += 1;
            }
        }
        if let Some(elo) = event.elo
            && decided_matches > 0
        {
            candidates.push(TemporalCandidate {
                date: event.date,
                elo,
                win_rate: wins as f64 / decided_matches as f64,
            });
        }
    }
    candidates
}

fn select_evenly_spaced(
    candidates: &[TemporalCandidate],
    first_date: i64,
    last_date: i64,
) -> Vec<TemporalCandidate> {
    if candidates.len() < WINRATE_POINTS_PER_PLAYER {
        return Vec::new();
    }

    let duration = last_date.saturating_sub(first_date).max(1);
    let interval_count = (WINRATE_POINTS_PER_PLAYER - 1) as i64;
    let target_dates: Vec<_> = (0..WINRATE_POINTS_PER_PLAYER)
        .map(|index| {
            first_date.saturating_add(duration.saturating_mul(index as i64) / interval_count)
        })
        .collect();
    let candidate_count = candidates.len();
    let mut costs = vec![vec![f64::INFINITY; candidate_count]; WINRATE_POINTS_PER_PLAYER];
    let mut previous = vec![vec![None; candidate_count]; WINRATE_POINTS_PER_PLAYER];

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        costs[0][candidate_index] = candidate.date.abs_diff(target_dates[0]) as f64;
    }
    for target_index in 1..WINRATE_POINTS_PER_PLAYER {
        for candidate_index in target_index..candidate_count {
            for previous_index in target_index - 1..candidate_index {
                let prior_cost = costs[target_index - 1][previous_index];
                if !prior_cost.is_finite() {
                    continue;
                }
                let distance = candidates[candidate_index]
                    .date
                    .abs_diff(target_dates[target_index]) as f64;
                let cost = prior_cost + distance;
                if cost < costs[target_index][candidate_index] {
                    costs[target_index][candidate_index] = cost;
                    previous[target_index][candidate_index] = Some(previous_index);
                }
            }
        }
    }

    let Some((mut candidate_index, _)) = costs[WINRATE_POINTS_PER_PLAYER - 1]
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .filter(|(_, cost)| cost.is_finite())
    else {
        return Vec::new();
    };
    let mut selected = Vec::with_capacity(WINRATE_POINTS_PER_PLAYER);
    for target_index in (0..WINRATE_POINTS_PER_PLAYER).rev() {
        selected.push(candidates[candidate_index]);
        if target_index > 0 {
            candidate_index = previous[target_index][candidate_index]
                .expect("a finite assignment has a preceding candidate");
        }
    }
    selected.reverse();
    selected
}

/// Players whose win rate is based on enough decided matches to chart.
pub(crate) fn eligible_winrate_players<'a>(players: &[&'a PlayerSeason]) -> Vec<&'a PlayerSeason> {
    players
        .iter()
        .copied()
        .filter(|player| {
            player.games_played >= MIN_GAMES_FOR_WINRATE_PLOTS && player.win_rate().is_some()
        })
        .collect()
}

/// UUIDs of the players whose win rate is based on enough decided matches.
pub(crate) fn eligible_winrate_uuids(players: &[&PlayerSeason]) -> HashSet<String> {
    eligible_winrate_players(players)
        .into_iter()
        .map(|player| player.uuid.clone())
        .collect()
}

/// Inclusive Elo range covering every supplied value.
pub(crate) fn elo_range(
    values: impl IntoIterator<Item = i64>,
) -> Result<(i64, i64), Box<dyn Error>> {
    let mut bounds: Option<(i64, i64)> = None;
    for value in values {
        bounds = Some(match bounds {
            Some((min, max)) => (min.min(value), max.max(value)),
            None => (value, value),
        });
    }
    bounds.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot").into())
}

/// One season's matches, split by the UTC hour their match started in.
pub(crate) struct SeasonHours {
    pub(crate) season: u32,
    /// Non-decayed matches, forfeits included, per UTC hour of match start.
    pub(crate) matches: [u64; HOURS_PER_DAY],
    /// How many of those matches carry the source forfeit flag.
    pub(crate) forfeited_matches: u64,
    /// Every player's share of those matches, per UTC hour of match start.
    pub(crate) players: HashMap<String, [u64; HOURS_PER_DAY]>,
}

impl SeasonHours {
    /// Every non-decayed match in the season, forfeits included.
    pub(crate) fn match_count(&self) -> u64 {
        self.matches.iter().sum()
    }
}

#[derive(Default)]
struct SeasonHourTotals {
    matches: [u64; HOURS_PER_DAY],
    forfeited_matches: u64,
    players: HashMap<String, [u64; HOURS_PER_DAY]>,
}

#[derive(Default)]
struct SeasonHoursChunk {
    by_season: BTreeMap<u32, SeasonHourTotals>,
}

/// The UTC hour of day, 0 through 23, holding a Unix-seconds timestamp.
fn utc_hour_of_day(timestamp: i64) -> usize {
    let seconds_into_day = timestamp.rem_euclid(SECONDS_PER_DAY);
    usize::try_from(seconds_into_day / SECONDS_PER_HOUR).expect("a day holds 24 UTC hours")
}

/// Reads the selected files into per-UTC-hour match counts, by season.
///
/// These counts describe when matches are played rather than how they ended, so
/// forfeited matches are kept and only decayed rows are dropped. That deviates
/// from [`passes_default_filters`], so the analyses that read these counts have
/// to report how many forfeited matches they counted.
pub(crate) fn load_season_hours<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
    seasons: Option<&[u32]>,
) -> Result<Vec<SeasonHours>, Box<dyn Error>> {
    let mut combined = SeasonHoursChunk::default();
    for path in paths {
        let reader = open(path)?;
        let projection = compatible_projection(&reader, HOUR_OF_DAY_PROJECTION)?;
        let chunks: Result<Vec<_>, io::Error> = (0..reader.num_row_groups())
            .into_par_iter()
            .filter(|&index| row_group_matches(&reader, index, seasons))
            .map(|index| {
                let worker_reader =
                    open(path).map_err(|error| io::Error::other(error.to_string()))?;
                let row_group = worker_reader
                    .get_row_group(index)
                    .map_err(io::Error::other)?;
                let mut chunk = SeasonHoursChunk::default();
                for row in row_group
                    .get_row_iter(Some(projection.clone()))
                    .map_err(io::Error::other)?
                {
                    chunk.add_row(&row.map_err(io::Error::other)?, seasons)?;
                }
                Ok(chunk)
            })
            .collect();
        for chunk in chunks? {
            combined.merge(chunk);
        }
    }

    if combined.by_season.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no non-decayed matches found in selected files",
        )
        .into());
    }

    Ok(combined
        .by_season
        .into_iter()
        .map(|(season, totals)| SeasonHours {
            season,
            matches: totals.matches,
            forfeited_matches: totals.forfeited_matches,
            players: totals.players,
        })
        .collect())
}

impl SeasonHoursChunk {
    /// Adds one match to the hour it started in, for its season and for every
    /// player it contains. A repeated UUID in one match counts as one
    /// appearance.
    fn add_row(&mut self, row: &Row, seasons: Option<&[u32]>) -> io::Result<()> {
        if boolean_field(row, "decayed")? != Some(false) {
            return Ok(());
        }

        let season = integer_field(row, "season")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing season"))?;
        let season = u32::try_from(season).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
        })?;
        if seasons.is_some_and(|requested| !requested.contains(&season)) {
            return Ok(());
        }

        let timestamp = integer_field(row, "date")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing match date"))?;
        let hour = utc_hour_of_day(timestamp);
        let totals = self.by_season.entry(season).or_default();
        totals.matches[hour] = totals.matches[hour].saturating_add(1);
        if boolean_field(row, "forfeited")? == Some(true) {
            totals.forfeited_matches = totals.forfeited_matches.saturating_add(1);
        }

        let Some(participants) = list_field(row, "players")? else {
            return Ok(());
        };
        let mut uuids: Vec<&str> = Vec::new();
        for participant in participants.elements() {
            let Field::Group(participant) = participant else {
                return Err(invalid_field("player entry", participant));
            };
            if let Some(uuid) = string_field(participant, "uuid")?
                && !uuids.contains(&uuid)
            {
                uuids.push(uuid);
            }
        }
        for uuid in uuids {
            let hours = totals.players.entry(uuid.to_owned()).or_default();
            hours[hour] = hours[hour].saturating_add(1);
        }
        Ok(())
    }

    fn merge(&mut self, chunk: Self) {
        for (season, incoming) in chunk.by_season {
            let merged = self.by_season.entry(season).or_default();
            for (hour, matches) in incoming.matches.into_iter().enumerate() {
                merged.matches[hour] = merged.matches[hour].saturating_add(matches);
            }
            merged.forfeited_matches = merged
                .forfeited_matches
                .saturating_add(incoming.forfeited_matches);
            for (uuid, hours) in incoming.players {
                let merged_hours = merged.players.entry(uuid).or_default();
                for (hour, matches) in hours.into_iter().enumerate() {
                    merged_hours[hour] = merged_hours[hour].saturating_add(matches);
                }
            }
        }
    }
}

/// Timeline milestones the in-game analyses compare. Each is the first time a
/// player's timeline records the matching source event type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Milestone {
    EnterNether,
    FindBastion,
    FindFortress,
    BlindTravel,
    FollowEnderEye,
    EnterEnd,
}

/// Every milestone in run order.
pub(crate) const MILESTONES: [Milestone; 6] = [
    Milestone::EnterNether,
    Milestone::FindBastion,
    Milestone::FindFortress,
    Milestone::BlindTravel,
    Milestone::FollowEnderEye,
    Milestone::EnterEnd,
];

impl Milestone {
    fn event_type(self) -> &'static str {
        match self {
            Self::EnterNether => "story.enter_the_nether",
            Self::FindBastion => "nether.find_bastion",
            Self::FindFortress => "nether.find_fortress",
            Self::BlindTravel => "projectelo.timeline.blind_travel",
            Self::FollowEnderEye => "story.follow_ender_eye",
            Self::EnterEnd => "story.enter_the_end",
        }
    }

    /// Short chart label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::EnterNether => "Enter Nether",
            Self::FindBastion => "Bastion",
            Self::FindFortress => "Fortress",
            Self::BlindTravel => "Blind travel",
            Self::FollowEnderEye => "Stronghold",
            Self::EnterEnd => "Enter End",
        }
    }
}

const DEATH_EVENT: &str = "projectelo.timeline.death";

/// Overworld structures the seed generator offers.
pub(crate) const SEED_TYPES: [&str; 5] = [
    "VILLAGE",
    "SHIPWRECK",
    "DESERT_TEMPLE",
    "BURIED_TREASURE",
    "RUINED_PORTAL",
];

/// Bastion layouts the seed generator offers.
pub(crate) const BASTION_TYPES: [&str; 4] = ["BRIDGE", "HOUSING", "STABLES", "TREASURE"];

/// One player's side of a two-player match.
#[derive(Clone, Debug)]
pub(crate) struct DuelSide {
    /// The 32-hex-digit player UUID packed into one integer.
    pub(crate) uuid: u128,
    /// Rating going into the match, from `changes[].eloRate`.
    pub(crate) elo_before: Option<i32>,
    /// Signed rating change the match applied, from `changes[].change`.
    pub(crate) elo_change: Option<i32>,
    /// Lowercase two-letter country code, when the profile lists one.
    pub(crate) country: Option<[u8; 2]>,
    /// First match-clock time, in milliseconds, of each [`MILESTONES`] entry.
    pub(crate) splits: [Option<NonZeroU32>; MILESTONES.len()],
    /// Timeline death events.
    pub(crate) deaths: u16,
}

/// One non-decayed, non-forfeited match between two distinct players.
#[derive(Clone, Debug)]
pub(crate) struct Duel {
    pub(crate) id: i64,
    /// Match start in Unix seconds.
    pub(crate) date: i64,
    pub(crate) season: u32,
    /// `result.time` in milliseconds.
    pub(crate) duration_ms: Option<u32>,
    /// Index into `sides` of the recorded winner, when the winner took part.
    pub(crate) winner: Option<usize>,
    /// Index into [`SEED_TYPES`].
    pub(crate) seed_type: Option<usize>,
    /// Index into [`BASTION_TYPES`].
    pub(crate) bastion_type: Option<usize>,
    /// Whether any timeline event was read for the match.
    pub(crate) has_timeline: bool,
    pub(crate) sides: [DuelSide; 2],
}

impl Duel {
    /// Whether `side` won, or `None` for a match without a recorded winner.
    pub(crate) fn won(&self, side: usize) -> Option<bool> {
        self.winner.map(|winner| winner == side)
    }

    /// Rating after the match, when both the rating and its change are known.
    pub(crate) fn elo_after(&self, side: usize) -> Option<i32> {
        let side = &self.sides[side];
        Some(side.elo_before? + side.elo_change?)
    }

    /// Both pre-match ratings, when both are known.
    pub(crate) fn elos(&self) -> Option<(i32, i32)> {
        Some((self.sides[0].elo_before?, self.sides[1].elo_before?))
    }

    /// Elo-predicted score of `side`, from both pre-match ratings.
    pub(crate) fn expected_score(&self, side: usize) -> Option<f64> {
        let (first, second) = self.elos()?;
        let (own, other) = if side == 0 {
            (first, second)
        } else {
            (second, first)
        };
        Some(expected_score(own, other))
    }
}

/// Logistic Elo expectation on the conventional 400-point scale.
pub(crate) fn expected_score(own: i32, other: i32) -> f64 {
    1.0 / (1.0 + 10f64.powf(f64::from(other - own) / 400.0))
}

/// Which optional, costly columns a duel load reads.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DuelColumns {
    /// Timeline splits and deaths.
    pub(crate) timelines: bool,
    /// The latest nickname per player and season.
    pub(crate) nicknames: bool,
}

/// Every selected two-player match in (date, id) order.
pub(crate) struct Duels {
    pub(crate) duels: Vec<Duel>,
    /// Latest nickname per (season, uuid), when requested.
    pub(crate) nicknames: HashMap<(u32, u128), String>,
}

/// A player's appearance in one duel.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Appearance {
    pub(crate) duel: usize,
    pub(crate) side: usize,
}

impl Duels {
    /// Every player's appearances per season, in match order.
    pub(crate) fn appearances(&self) -> HashMap<(u32, u128), Vec<Appearance>> {
        let mut by_player: HashMap<(u32, u128), Vec<Appearance>> = HashMap::new();
        for (index, duel) in self.duels.iter().enumerate() {
            for (side, player) in duel.sides.iter().enumerate() {
                by_player
                    .entry((duel.season, player.uuid))
                    .or_default()
                    .push(Appearance { duel: index, side });
            }
        }
        by_player
    }

    /// Every selected season, ascending.
    pub(crate) fn seasons(&self) -> Vec<u32> {
        self.duels
            .iter()
            .map(|duel| duel.season)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Final and peak post-match ratings for every player with a rated match.
    pub(crate) fn ratings(&self) -> HashMap<(u32, u128), SeasonRating> {
        let mut ratings: HashMap<(u32, u128), SeasonRating> = HashMap::new();
        for duel in &self.duels {
            for side in 0..2 {
                let Some(after) = duel.elo_after(side) else {
                    continue;
                };
                let rating = ratings
                    .entry((duel.season, duel.sides[side].uuid))
                    .or_insert(SeasonRating {
                        final_elo: after,
                        peak_elo: after,
                        rated_matches: 0,
                    });
                rating.final_elo = after;
                rating.peak_elo = rating.peak_elo.max(after);
                rating.rated_matches += 1;
            }
        }
        ratings
    }
}

/// One player's ratings over a season.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SeasonRating {
    pub(crate) final_elo: i32,
    pub(crate) peak_elo: i32,
    pub(crate) rated_matches: u32,
}

#[derive(Default)]
struct DuelChunk {
    duels: Vec<Duel>,
    nicknames: HashMap<(u32, u128), (i64, String)>,
}

const DUEL_COLUMNS: [&str; 20] = [
    "id",
    "date",
    "season",
    "decayed",
    "forfeited",
    "seedType",
    "bastionType",
    "result.time",
    "result.uuid",
    "changes.list.element.uuid",
    "changes.list.element.change",
    "changes.list.element.eloRate",
    "players.list.element.uuid",
    "players.list.element.country",
    "players.list.element.nickname",
    "timelines.list.element.time",
    "timelines.list.element.type",
    "timelines.list.element.uuid",
    "seed.overworld",
    "seed.nether",
];

/// Keeps the parts of a file schema named by dotted `paths`, so one projection
/// works across season files whose columns differ in presence or type.
fn project_schema(schema: &Type, paths: &[Vec<&str>], depth: usize) -> Option<Type> {
    let fields: Vec<TypePtr> = schema
        .get_fields()
        .iter()
        .filter_map(|field| {
            let matching: Vec<Vec<&str>> = paths
                .iter()
                .filter(|path| path.get(depth) == Some(&field.name()))
                .cloned()
                .collect();
            if matching.is_empty() {
                return None;
            }
            if field.is_primitive() {
                return Some(field.clone());
            }
            if matching.iter().any(|path| path.len() == depth + 1) {
                return Some(field.clone());
            }
            project_schema(field, &matching, depth + 1).map(Arc::new)
        })
        .collect();
    if fields.is_empty() {
        return None;
    }
    let info = schema.get_basic_info();
    let mut builder = Type::group_type_builder(info.name())
        .with_fields(fields)
        .with_converted_type(info.converted_type())
        .with_logical_type(info.logical_type_ref().cloned());
    if info.has_repetition() {
        builder = builder.with_repetition(info.repetition());
    }
    builder.build().ok()
}

/// Select requested column paths while retaining this file's root name,
/// integer annotations and historical numeric types. DuckDB and the older
/// exporter use different but equivalent Parquet schema annotations.
pub(crate) fn compatible_projection(
    reader: &ParquetFile,
    specification: &str,
) -> Result<Type, Box<dyn Error>> {
    fn leaves<'a>(node: &'a Type, prefix: &mut Vec<&'a str>, paths: &mut Vec<Vec<&'a str>>) {
        for field in node.get_fields() {
            prefix.push(field.name());
            if field.is_primitive() {
                paths.push(prefix.clone());
            } else {
                leaves(field, prefix, paths);
            }
            prefix.pop();
        }
    }
    let requested = parse_message_type(specification)?;
    let mut paths = Vec::new();
    leaves(&requested, &mut Vec::new(), &mut paths);
    project_schema(reader.metadata().file_metadata().schema(), &paths, 0)
        .ok_or_else(|| io::Error::other("none of the requested columns exist").into())
}

fn duel_projection(schema: &Type, columns: DuelColumns) -> Result<Type, Box<dyn Error>> {
    let paths: Vec<Vec<&str>> = DUEL_COLUMNS
        .iter()
        .filter(|path| columns.timelines || !path.starts_with("timelines."))
        .filter(|path| columns.nicknames || !path.ends_with(".nickname"))
        .map(|path| path.split('.').collect())
        .collect();
    project_schema(schema, &paths, 0).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "file has none of the match columns",
        )
        .into()
    })
}

/// Reads a whole number stored as any integer or floating Parquet type.
fn loose_integer(field: &Field) -> Option<i64> {
    match field {
        Field::Long(value) => Some(*value),
        Field::Int(value) => Some(i64::from(*value)),
        Field::Short(value) => Some(i64::from(*value)),
        Field::Double(value) if value.is_finite() => Some(value.round() as i64),
        Field::Float(value) if value.is_finite() => Some(f64::from(*value).round() as i64),
        _ => None,
    }
}

fn loose_str(field: &Field) -> Option<&str> {
    match field {
        Field::Str(value) => Some(value),
        _ => None,
    }
}

fn loose_group(field: &Field) -> Option<&Row> {
    match field {
        Field::Group(row) => Some(row),
        _ => None,
    }
}

fn loose_list(field: &Field) -> &[Field] {
    match field {
        Field::ListInternal(list) => list.elements(),
        _ => &[],
    }
}

fn parse_uuid(value: &str) -> Option<u128> {
    (value.len() == 32)
        .then(|| u128::from_str_radix(value, 16).ok())
        .flatten()
}

fn to_i32(value: i64) -> Option<i32> {
    i32::try_from(value).ok()
}

impl DuelChunk {
    fn add_row(&mut self, row: &Row, seasons: Option<&[u32]>, columns: DuelColumns) {
        let mut id = 0;
        let mut date = None;
        let mut season = None;
        let mut decayed = None;
        let mut forfeited = None;
        let mut seed_type = None;
        let mut bastion_type = None;
        let mut result = None;
        let mut changes: &[Field] = &[];
        let mut players: &[Field] = &[];
        let mut timelines: &[Field] = &[];
        let mut seed = None;

        for (name, value) in row.get_column_iter() {
            match name.as_str() {
                "id" => id = loose_integer(value).unwrap_or_default(),
                "date" => date = loose_integer(value),
                "season" => season = loose_integer(value),
                "decayed" => decayed = matches!(value, Field::Bool(flag) if !flag).then_some(()),
                "forfeited" => {
                    forfeited = matches!(value, Field::Bool(flag) if !flag).then_some(());
                }
                "seedType" => seed_type = loose_str(value),
                "bastionType" => bastion_type = loose_str(value),
                "result" => result = loose_group(value),
                "changes" => changes = loose_list(value),
                "players" => players = loose_list(value),
                "timelines" => timelines = loose_list(value),
                "seed" => seed = loose_group(value),
                _ => {}
            }
        }
        // Rows pass only when both flags are present and false.
        if decayed.is_none() || forfeited.is_none() {
            return;
        }
        let (Some(date), Some(season)) = (date, season.and_then(|s| u32::try_from(s).ok())) else {
            return;
        };
        if seasons.is_some_and(|requested| !requested.contains(&season)) {
            return;
        }

        let mut sides: Vec<DuelSide> = Vec::with_capacity(2);
        for player in players.iter().filter_map(loose_group) {
            let mut uuid = None;
            let mut country = None;
            let mut nickname = None;
            for (name, value) in player.get_column_iter() {
                match name.as_str() {
                    "uuid" => uuid = loose_str(value).and_then(parse_uuid),
                    "country" => {
                        country = loose_str(value)
                            .filter(|code| code.len() == 2)
                            .map(|code| [code.as_bytes()[0], code.as_bytes()[1]]);
                    }
                    "nickname" => nickname = loose_str(value),
                    _ => {}
                }
            }
            let Some(uuid) = uuid else { continue };
            if sides.iter().any(|side| side.uuid == uuid) {
                continue;
            }
            if columns.nicknames
                && let Some(nickname) = nickname
            {
                let entry = self
                    .nicknames
                    .entry((season, uuid))
                    .or_insert_with(|| (date, nickname.to_owned()));
                if date >= entry.0 {
                    *entry = (date, nickname.to_owned());
                }
            }
            sides.push(DuelSide {
                uuid,
                elo_before: None,
                elo_change: None,
                country,
                splits: [None; MILESTONES.len()],
                deaths: 0,
            });
        }
        let Ok(mut sides) = <[DuelSide; 2]>::try_from(sides) else {
            return;
        };
        let uuids = [sides[0].uuid, sides[1].uuid];
        let side_of = |uuid: Option<u128>| {
            uuid.and_then(|uuid| uuids.iter().position(|known| *known == uuid))
        };

        for change in changes.iter().filter_map(loose_group) {
            let mut uuid = None;
            let mut delta = None;
            let mut elo = None;
            for (name, value) in change.get_column_iter() {
                match name.as_str() {
                    "uuid" => uuid = loose_str(value).and_then(parse_uuid),
                    "change" => delta = loose_integer(value).and_then(to_i32),
                    "eloRate" => elo = loose_integer(value).and_then(to_i32),
                    _ => {}
                }
            }
            if let Some(index) = side_of(uuid) {
                sides[index].elo_before = elo;
                sides[index].elo_change = delta;
            }
        }

        let mut duration_ms = None;
        let mut winner = None;
        if let Some(result) = result {
            for (name, value) in result.get_column_iter() {
                match name.as_str() {
                    "time" => {
                        duration_ms =
                            loose_integer(value).and_then(|time| u32::try_from(time).ok());
                    }
                    "uuid" => winner = side_of(loose_str(value).and_then(parse_uuid)),
                    _ => {}
                }
            }
        }

        let mut has_timeline = false;
        for event in timelines.iter().filter_map(loose_group) {
            let mut time = None;
            let mut kind = None;
            let mut uuid = None;
            for (name, value) in event.get_column_iter() {
                match name.as_str() {
                    "time" => time = loose_integer(value).and_then(|t| u32::try_from(t).ok()),
                    "type" => kind = loose_str(value),
                    "uuid" => uuid = loose_str(value).and_then(parse_uuid),
                    _ => {}
                }
            }
            has_timeline = true;
            let (Some(index), Some(kind)) = (side_of(uuid), kind) else {
                continue;
            };
            if kind == DEATH_EVENT {
                sides[index].deaths = sides[index].deaths.saturating_add(1);
                continue;
            }
            let Some(milestone) = MILESTONES.iter().position(|m| m.event_type() == kind) else {
                continue;
            };
            if let Some(time) = time.and_then(NonZeroU32::new) {
                let split = &mut sides[index].splits[milestone];
                *split = Some(split.map_or(time, |previous| previous.min(time)));
            }
        }

        // Older files leave `seedType` empty; fall back to the seed's own labels.
        let mut overworld = None;
        let mut nether = None;
        if let Some(seed) = seed {
            for (name, value) in seed.get_column_iter() {
                match name.as_str() {
                    "overworld" => overworld = loose_str(value),
                    "nether" => nether = loose_str(value),
                    _ => {}
                }
            }
        }
        let seed_type = seed_type
            .or(overworld)
            .and_then(|kind| SEED_TYPES.iter().position(|known| *known == kind));
        let bastion_type = bastion_type
            .or(nether)
            .and_then(|kind| BASTION_TYPES.iter().position(|known| *known == kind));

        self.duels.push(Duel {
            id,
            date,
            season,
            duration_ms,
            winner,
            seed_type,
            bastion_type,
            has_timeline,
            sides,
        });
    }
}

/// Reads every non-decayed, non-forfeited two-player match of the selected
/// files, in (date, id) order. Row groups of every file are read in parallel.
pub(crate) fn load_duels<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
    seasons: Option<&[u32]>,
    columns: DuelColumns,
) -> Result<Duels, Box<dyn Error>> {
    let mut work = Vec::new();
    for path in paths {
        let reader = open(path)?;
        let projection = duel_projection(reader.metadata().file_metadata().schema(), columns)?;
        for index in 0..reader.num_row_groups() {
            if row_group_matches(&reader, index, seasons) {
                work.push((path, index, projection.clone()));
            }
        }
    }

    let chunks: Result<Vec<DuelChunk>, io::Error> = work
        .into_par_iter()
        .map(|(path, index, projection)| {
            let reader = open(path).map_err(|error| io::Error::other(error.to_string()))?;
            let row_group = reader.get_row_group(index).map_err(io::Error::other)?;
            let mut chunk = DuelChunk::default();
            for row in row_group
                .get_row_iter(Some(projection))
                .map_err(io::Error::other)?
            {
                chunk.add_row(&row.map_err(io::Error::other)?, seasons, columns);
            }
            Ok(chunk)
        })
        .collect();

    let mut duels = Vec::new();
    let mut nicknames: HashMap<(u32, u128), (i64, String)> = HashMap::new();
    for chunk in chunks? {
        duels.extend(chunk.duels);
        for (key, (date, nickname)) in chunk.nicknames {
            match nicknames.get(&key) {
                Some((latest, _)) if *latest > date => {}
                _ => {
                    nicknames.insert(key, (date, nickname));
                }
            }
        }
    }
    if duels.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no non-decayed, non-forfeited two-player matches in the selected files",
        )
        .into());
    }
    duels.par_sort_unstable_by_key(|duel| (duel.date, duel.id));

    Ok(Duels {
        duels,
        nicknames: nicknames
            .into_iter()
            .map(|(key, (_, nickname))| (key, nickname))
            .collect(),
    })
}

/// Linear-interpolated percentile of an already sorted, non-empty slice.
pub(crate) fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let position = fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = (position.ceil() as usize).min(sorted.len() - 1);
    let weight = position - lower as f64;
    sorted[lower] + (sorted[upper] - sorted[lower]) * weight
}

/// Median of an unsorted sample, or `None` when it is empty.
pub(crate) fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable_by(f64::total_cmp);
    Some(percentile(values, 0.5))
}

/// Idle time after a match ends that closes a player's playing session.
pub(crate) const SESSION_GAP_SECONDS: i64 = 30 * 60;

/// Where one appearance falls within its player's playing session.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SessionSlot {
    pub(crate) appearance: Appearance,
    /// Zero-based position within the session.
    pub(crate) position: usize,
    /// Whether the player played no further match within the session gap.
    pub(crate) last: bool,
}

/// Splits one player's ordered appearances into sessions: a new session
/// starts when a match begins more than [`SESSION_GAP_SECONDS`] after the
/// previous one ended.
pub(crate) fn session_slots(duels: &[Duel], appearances: &[Appearance]) -> Vec<SessionSlot> {
    let ends_session = |index: usize| {
        let Some(next) = appearances.get(index + 1) else {
            return true;
        };
        let current = &duels[appearances[index].duel];
        let ended = current.date + i64::from(current.duration_ms.unwrap_or(0) / 1000);
        duels[next.duel].date - ended > SESSION_GAP_SECONDS
    };
    let mut slots = Vec::with_capacity(appearances.len());
    let mut position = 0;
    for (index, appearance) in appearances.iter().enumerate() {
        let last = ends_session(index);
        slots.push(SessionSlot {
            appearance: *appearance,
            position,
            last,
        });
        position = if last { 0 } else { position + 1 };
    }
    slots
}

// Forfeit research is an explicitly requested exception to the default filters.
// Shared event measurements, cohorts and sufficient statistics live here so
// each registered plot can remain self-contained in its own analysis file.
use crate::analysis::AnalysisInput;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, BufWriter, Write};

pub(crate) const RESEARCH_RANKS: [&str; 7] = [
    "Coal",
    "Iron",
    "Gold",
    "Emerald",
    "Diamond",
    "Netherite",
    "Unrated",
];
pub(crate) fn research_rank(elo: Option<i32>) -> u8 {
    match elo {
        None => 6,
        Some(x) if x < 600 => 0,
        Some(x) if x < 900 => 1,
        Some(x) if x < 1200 => 2,
        Some(x) if x < 1500 => 3,
        Some(x) if x < 2000 => 4,
        Some(_) => 5,
    }
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub(crate) struct ResearchMoment {
    pub(crate) n: u64,
    pub(crate) sum: f64,
    pub(crate) sum_sq: f64,
}
impl ResearchMoment {
    pub(crate) fn add(&mut self, x: f64) {
        if x.is_finite() {
            self.n += 1;
            self.sum += x;
            self.sum_sq += x * x;
        }
    }
    pub(crate) fn mean(&self) -> Option<f64> {
        (self.n > 0).then(|| self.sum / self.n as f64)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct ResearchBlock {
    pub(crate) games: u64,
    pub(crate) quits: u64,
    pub(crate) wins: u64,
    pub(crate) deaths: u64,
    pub(crate) death_matches: u64,
    pub(crate) continued_after_death: u64,
    pub(crate) elo: ResearchMoment,
    pub(crate) delta: ResearchMoment,
    pub(crate) timed_delta: ResearchMoment,
    pub(crate) residual: ResearchMoment,
    pub(crate) minutes: f64,
    pub(crate) pace: ResearchMoment,
    pub(crate) nether: ResearchMoment,
    pub(crate) seed_pace: [ResearchMoment; 20],
}
impl ResearchBlock {
    pub(crate) fn quit_rate(&self) -> f64 {
        self.quits as f64 / self.games.max(1) as f64
    }
    pub(crate) fn elo_per_100(&self) -> Option<f64> {
        self.delta.mean().map(|x| x * 100.0)
    }
    pub(crate) fn elo_per_hour(&self) -> Option<f64> {
        (self.minutes > 0.0).then(|| self.timed_delta.sum * 60.0 / self.minutes)
    }
    /// Broader seed-type matching for short cross-season blocks; layout
    /// matching is retained separately in matched_pace_improvement.
    pub(crate) fn seed_type_pace_improvement(&self, later: &Self) -> Option<f64> {
        if self.pace.n < 10 || later.pace.n < 10 {
            return None;
        }
        let mut before = 0.0;
        let mut after = 0.0;
        let mut weight = 0.0;
        for (left, right) in self
            .seed_pace
            .as_chunks::<4>()
            .0
            .iter()
            .zip(later.seed_pace.as_chunks::<4>().0.iter())
        {
            let an = left.iter().map(|x| x.n).sum::<u64>();
            let bn = right.iter().map(|x| x.n).sum::<u64>();
            if an >= 2 && bn >= 2 {
                let w = an.min(bn) as f64;
                before += left.iter().map(|x| x.sum).sum::<f64>() / an as f64 * w;
                after += right.iter().map(|x| x.sum).sum::<f64>() / bn as f64 * w;
                weight += w;
            }
        }
        (weight >= 10.0 && before > 0.0).then(|| (before - after) / before)
    }
    pub(crate) fn matched_pace_improvement(&self, later: &Self) -> Option<f64> {
        if self.pace.n < 10 || later.pace.n < 10 {
            return None;
        }
        let mut before = 0.0;
        let mut after = 0.0;
        let mut weight = 0.0;
        for (a, b) in self.seed_pace.iter().zip(&later.seed_pace) {
            if a.n >= 2 && b.n >= 2 {
                let w = a.n.min(b.n) as f64;
                before += a.mean()? * w;
                after += b.mean()? * w;
                weight += w;
            }
        }
        (weight >= 10.0 && before > 0.0).then(|| (before - after) / before)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResearchProfile {
    pub(crate) season: u32,
    pub(crate) uuid: String,
    pub(crate) all_games: u64,
    pub(crate) rank: u8,
    pub(crate) total: ResearchBlock,
    pub(crate) first: ResearchBlock,
    pub(crate) last: ResearchBlock,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResearchWindow {
    pub(crate) season: u32,
    pub(crate) uuid: String,
    pub(crate) day: u32,
    pub(crate) rank: u8,
    pub(crate) before: ResearchBlock,
    pub(crate) after: ResearchBlock,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct ResearchAudit {
    pub(crate) season: u32,
    pub(crate) rows: u64,
    pub(crate) played: u64,
    pub(crate) flagged: u64,
    pub(crate) draws: u64,
    pub(crate) missing_timeline: u64,
    pub(crate) missing_duration: u64,
    pub(crate) excluded_death_resets: u64,
    pub(crate) post_forfeit_deaths: u64,
    pub(crate) unknown_quit: u64,
    pub(crate) explicit_quit: u64,
    pub(crate) skipped: u64,
    pub(crate) invalid_events: u64,
    pub(crate) duplicate_ids: u64,
    pub(crate) continuity_checked: u64,
    pub(crate) continuity_exact: u64,
    pub(crate) start: i64,
    pub(crate) end: i64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecoveryKey {
    pub(crate) season: u32, // zero is pooled across available seasons
    pub(crate) rank: u8,    // seven is pooled across ratings, including unrated
    pub(crate) kind: u8,
    pub(crate) view: u8, // 0 rank; 1 split; 2 split deficit; 3 minutes; 4 earlier opponent death
    pub(crate) bin: i8,
    pub(crate) behavior: u8, // 0 all deaths (reference); 1 no own forfeit
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RecoveryCell {
    pub(crate) key: RecoveryKey,
    pub(crate) n: u64,
    pub(crate) wins: u64,
    pub(crate) completion_wins: u64,
    pub(crate) players: u64,
    pub(crate) player_mean: f64,
    pub(crate) low: f64,
    pub(crate) high: f64,
    pub(crate) elo: ResearchMoment,
    pub(crate) remaining_minutes: f64,
}
impl RecoveryCell {
    pub(crate) fn rate(&self) -> f64 {
        self.wins as f64 / self.n.max(1) as f64
    }
}
#[derive(Clone, Copy, Default)]
struct RecoveryTally {
    n: u64,
    wins: u64,
    completion_wins: u64,
    elo: ResearchMoment,
    remaining_minutes: f64,
}
impl RecoveryTally {
    fn merge(&mut self, other: &Self) {
        self.n += other.n;
        self.wins += other.wins;
        self.completion_wins += other.completion_wins;
        self.elo.n += other.elo.n;
        self.elo.sum += other.elo.sum;
        self.elo.sum_sq += other.elo.sum_sq;
        self.remaining_minutes += other.remaining_minutes;
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResearchEfficiency {
    pub(crate) season: u32,
    pub(crate) uuid: String,
    pub(crate) rank: u8,
    pub(crate) action: u8, // 0 explicit quit after death; 1 no own forfeit after death
    pub(crate) stratum: i32,
    pub(crate) immediate: ResearchMoment,
    pub(crate) remaining_minutes: f64,
    pub(crate) horizons: u64,
    pub(crate) next_delta: f64,
    pub(crate) next_minutes: f64,
    pub(crate) session_horizons: u64,
    pub(crate) session_delta: f64,
    pub(crate) session_minutes: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResearchCalibration {
    pub(crate) season: u32,
    pub(crate) gap: u32,
    pub(crate) all: bool,
    pub(crate) n: u64,
    pub(crate) score: f64,
    pub(crate) expected: f64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ResearchData {
    pub(crate) audits: Vec<ResearchAudit>,
    pub(crate) recovery: Vec<RecoveryCell>,
    pub(crate) profiles: Vec<ResearchProfile>,
    pub(crate) windows: Vec<ResearchWindow>,
    pub(crate) efficiency: Vec<ResearchEfficiency>,
    pub(crate) calibration: Vec<ResearchCalibration>,
    pub(crate) opponent_after: Vec<OpponentAfterCell>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct OpponentAfterKey {
    pub(crate) rank: u8,
    pub(crate) behavior: u8,    // 0 all deaths (reference); 1 no own forfeit
    pub(crate) prior_death: u8, // 0 no earlier death; 1 earlier death; 2 either
    pub(crate) kind: u8,        // 0 all; 1 later death; 2 later forfeit; 3 neither;
                                // 4 death only; 5 forfeit only; 6 both; 7 no later death
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OpponentAfterCell {
    pub(crate) key: OpponentAfterKey,
    pub(crate) opportunities: u64,
    pub(crate) selected: u64,
    pub(crate) wins: u64,
    pub(crate) opportunity_players: u64,
    pub(crate) selected_players: u64,
    pub(crate) frequency_low: f64,
    pub(crate) frequency_high: f64,
    pub(crate) win_low: f64,
    pub(crate) win_high: f64,
}
impl OpponentAfterCell {
    pub(crate) fn frequency(&self) -> f64 {
        self.selected as f64 / self.opportunities.max(1) as f64
    }
    pub(crate) fn win_rate(&self) -> f64 {
        self.wins as f64 / self.selected.max(1) as f64
    }
}
#[derive(Clone, Copy, Default)]
struct OpponentAfterTally {
    opportunities: u64,
    selected: u64,
    wins: u64,
}
impl OpponentAfterTally {
    fn merge(&mut self, other: Self) {
        self.opportunities += other.opportunities;
        self.selected += other.selected;
        self.wins += other.wins;
    }
}
#[derive(Clone, Copy, Default)]
struct BinarySquares {
    success_sq: f64,
    success_trials: f64,
    trials_sq: f64,
    clusters: u64,
}
impl BinarySquares {
    fn add(&mut self, success: u64, trials: u64) {
        if trials > 0 {
            let s = success as f64;
            let n = trials as f64;
            self.success_sq += s * s;
            self.success_trials += s * n;
            self.trials_sq += n * n;
            self.clusters += 1;
        }
    }
    fn variance(&self, p: f64) -> f64 {
        (self.success_sq - 2.0 * p * self.success_trials + p * p * self.trials_sq).max(0.0)
            * self.clusters as f64
            / self.clusters.saturating_sub(1).max(1) as f64
    }
}
#[derive(Default)]
struct OpponentAfterMoments {
    frequency: BinarySquares,
    winning: BinarySquares,
}
impl OpponentAfterMoments {
    fn add(&mut self, t: OpponentAfterTally) {
        self.frequency.add(t.selected, t.opportunities);
        self.winning.add(t.wins, t.selected);
    }
}
#[derive(Default)]
struct OpponentAfterAggregate {
    tally: OpponentAfterTally,
    moments: OpponentAfterMoments,
}
fn binary_cluster_interval(
    success: u64,
    trials: u64,
    players: BinarySquares,
    matches: BinarySquares,
) -> (f64, f64) {
    if trials == 0 {
        return (0.0, 1.0);
    }
    let n = trials as f64;
    let p = success as f64 / n;
    let pv = players.variance(p);
    let mv = matches.variance(p);
    let independent = n * p * (1.0 - p);
    let variance = (pv + mv - independent).max(pv).max(mv).max(independent) / (n * n);
    let half = 1.96 * variance.sqrt();
    let z2 = 1.96f64.powi(2);
    let center = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let wilson = 1.96 * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt() / (1.0 + z2 / n);
    (
        (p - half).min(center - wilson).max(0.0),
        (p + half).max(center + wilson).min(1.0),
    )
}

#[derive(Clone, Copy, Default)]
struct ResearchEvent {
    time: u32,
    stage: u8,
    deficit: i8,
    opponent_adverse: bool,
}
#[derive(Clone, Default)]
struct ResearchSide {
    uuid: u128,
    elo: Option<i32>,
    delta: Option<i32>,
    completion: Option<u32>,
    quit: Option<u32>,
    events: [Option<ResearchEvent>; 1],
    deaths: u16,
    last_death: Option<u32>,
    nether: Option<u32>,
}
#[derive(Clone)]
struct ResearchMatch {
    id: i64,
    date: i64,
    season: u32,
    duration: u32,
    duration_valid: bool,
    flagged: bool,
    winner: Option<usize>,
    seed: Option<usize>,
    timeline: bool,
    invalid_events: u64,
    excluded_death_resets: u64,
    post_forfeit_deaths: u64,
    opponent_event_end: u32,
    sides: [ResearchSide; 2],
}

fn research_field<'a>(row: &'a Row, name: &str) -> Option<&'a Field> {
    row.get_column_iter()
        .find(|(n, _)| n.as_str() == name)
        .map(|(_, x)| x)
}
fn research_int(row: &Row, name: &str) -> Option<i64> {
    research_field(row, name).and_then(loose_integer)
}
fn research_str<'a>(row: &'a Row, name: &str) -> Option<&'a str> {
    research_field(row, name).and_then(loose_str)
}
fn research_list<'a>(row: &'a Row, name: &str) -> &'a [Field] {
    research_field(row, name)
        .map(loose_list)
        .unwrap_or_default()
}
fn research_match(row: &Row, requested: Option<&[u32]>) -> Option<ResearchMatch> {
    if !matches!(research_field(row, "decayed"), Some(Field::Bool(false)))
        || research_int(row, "type") != Some(2)
    {
        return None;
    }
    let season = u32::try_from(research_int(row, "season")?).ok()?;
    if requested.is_some_and(|x| !x.contains(&season)) {
        return None;
    }
    let flagged = match research_field(row, "forfeited")? {
        Field::Bool(x) => *x,
        _ => return None,
    };
    let result = research_field(row, "result").and_then(loose_group)?;
    let recorded_duration = research_int(result, "time")
        .and_then(|x| u32::try_from(x).ok())
        .filter(|x| *x > 0);
    let duration_valid = recorded_duration.is_some();
    // Missing-clock draws are real matches. The fallback is only an event
    // validation bound, never an observed duration for efficiency estimates.
    let duration = recorded_duration.unwrap_or_else(|| {
        research_list(row, "timelines")
            .iter()
            .filter_map(loose_group)
            .filter(|e| research_str(e, "type") != Some("projectelo.timeline.reset"))
            .filter_map(|e| research_int(e, "time").and_then(|x| u32::try_from(x).ok()))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    });
    let sides: Vec<_> = research_list(row, "players")
        .iter()
        .filter_map(loose_group)
        .filter_map(|p| research_str(p, "uuid").and_then(parse_uuid))
        .map(|uuid| ResearchSide {
            uuid,
            ..ResearchSide::default()
        })
        .collect();
    if sides.len() != 2 || sides[0].uuid == sides[1].uuid {
        return None;
    }
    let mut sides: [ResearchSide; 2] = sides.try_into().ok()?;
    let side_of = |uuid: Option<&str>| {
        uuid.and_then(parse_uuid)
            .and_then(|x| sides.iter().position(|s| s.uuid == x))
    };
    let winner = side_of(research_str(result, "uuid"));
    if research_str(result, "uuid").is_some() && winner.is_none() {
        return None;
    }
    for change in research_list(row, "changes").iter().filter_map(loose_group) {
        let index = research_str(change, "uuid")
            .and_then(parse_uuid)
            .and_then(|x| sides.iter().position(|s| s.uuid == x));
        if let Some(i) = index {
            sides[i].elo = research_int(change, "eloRate").and_then(to_i32);
            sides[i].delta = research_int(change, "change").and_then(to_i32);
        }
    }
    for completion in research_list(row, "completions")
        .iter()
        .filter_map(loose_group)
    {
        let index = research_str(completion, "uuid")
            .and_then(parse_uuid)
            .and_then(|x| sides.iter().position(|s| s.uuid == x));
        if let (Some(i), Some(t)) = (
            index,
            research_int(completion, "time")
                .and_then(|x| u32::try_from(x).ok())
                .filter(|x| *x > 0 && *x <= duration),
        ) {
            sides[i].completion = Some(sides[i].completion.map_or(t, |old| old.min(t)));
        }
    }
    let mut events = Vec::new();
    let mut invalid_events = 0;
    let mut excluded_death_resets = 0;
    for e in research_list(row, "timelines")
        .iter()
        .filter_map(loose_group)
    {
        let index = research_str(e, "uuid")
            .and_then(parse_uuid)
            .and_then(|x| sides.iter().position(|s| s.uuid == x));
        let time = research_int(e, "time").and_then(|x| u32::try_from(x).ok());
        let (Some(i), Some(t), Some(kind)) = (index, time, research_str(e, "type")) else {
            invalid_events += 1;
            continue;
        };
        if t > duration {
            invalid_events += 1;
            continue;
        }
        if kind == "projectelo.timeline.death_spawnpoint" {
            excluded_death_resets += 1;
            continue;
        }
        let code = match kind {
            "projectelo.timeline.death" => 0,
            "projectelo.timeline.forfeit" => 3,
            "story.enter_the_nether" => 4,
            "nether.find_bastion" => 5,
            "nether.find_fortress" => 6,
            "projectelo.timeline.blind_travel" => 7,
            "story.follow_ender_eye" => 8,
            "story.enter_the_end" => 9,
            _ => continue,
        };
        events.push((t, i, code));
    }
    events.sort_unstable();
    // A forfeit ends the shared match. Never treat later recorded events as
    // opportunities observed while the match was still being played.
    let first_forfeit = events.iter().find(|(_, _, code)| *code == 3).map(|e| e.0);
    let opponent_event_end = first_forfeit
        .into_iter()
        .chain(sides.iter().filter_map(|s| s.completion))
        .fold(duration, u32::min);
    let mut post_forfeit_deaths = 0;
    let mut stages = [0u8; 2];
    let mut adverse = [false; 2];
    for (t, i, code) in events {
        post_forfeit_deaths += u64::from(code == 0 && first_forfeit.is_some_and(|q| t > q));
        if t > opponent_event_end {
            continue;
        }
        if code == 3 {
            sides[i].quit = Some(sides[i].quit.map_or(t, |old| old.min(t)));
        } else if code == 0 {
            if t < opponent_event_end {
                let e = ResearchEvent {
                    time: t,
                    stage: stages[i],
                    deficit: stages[1 - i] as i8 - stages[i] as i8,
                    opponent_adverse: adverse[1 - i],
                };
                sides[i].events[code].get_or_insert(e);
                adverse[i] = true;
            }
            sides[i].deaths = sides[i].deaths.saturating_add(1);
            sides[i].last_death = Some(t);
        } else {
            stages[i] = stages[i].max((code - 3) as u8);
            if code == 4 && sides[i].nether.is_none() {
                sides[i].nether = Some(t);
            }
        }
    }
    let seed_group = research_field(row, "seed").and_then(loose_group);
    let seed_type = research_str(row, "seedType")
        .or_else(|| seed_group.and_then(|x| research_str(x, "overworld")));
    let bastion_type = research_str(row, "bastionType")
        .or_else(|| seed_group.and_then(|x| research_str(x, "nether")));
    let seed = seed_type
        .and_then(|x| SEED_TYPES.iter().position(|y| *y == x))
        .zip(bastion_type.and_then(|x| BASTION_TYPES.iter().position(|y| *y == x)))
        .map(|(a, b)| a * 4 + b);
    Some(ResearchMatch {
        id: research_int(row, "id")?,
        date: research_int(row, "date")?,
        season,
        duration,
        duration_valid,
        flagged,
        winner,
        seed,
        timeline: !research_list(row, "timelines").is_empty(),
        invalid_events,
        excluded_death_resets,
        post_forfeit_deaths,
        opponent_event_end,
        sides,
    })
}

fn research_block(history: &[(&ResearchMatch, usize)]) -> ResearchBlock {
    let mut b = ResearchBlock::default();
    for (m, i) in history {
        let s = &m.sides[*i];
        b.games += 1;
        b.quits += u64::from(s.quit.is_some());
        b.wins += u64::from(m.winner == Some(*i));
        b.deaths += u64::from(s.deaths > 0);
        b.death_matches += u64::from(s.events[0].is_some());
        b.continued_after_death += u64::from(s.events[0].is_some() && s.quit.is_none());
        if let Some(elo) = s.elo {
            b.elo.add(f64::from(elo));
        }
        if let Some(delta) = s.delta {
            b.delta.add(f64::from(delta));
        }
        if let (Some(own), Some(other)) = (s.elo, m.sides[1 - *i].elo) {
            let score = m.winner.map_or(0.5, |w| f64::from(w == *i));
            b.residual.add(score - expected_score(own, other));
        }
        if m.duration_valid {
            b.minutes += f64::from(m.duration) / 60_000.0;
            if let Some(delta) = s.delta {
                b.timed_delta.add(f64::from(delta));
            }
        }
        if let Some(t) = s.completion {
            b.pace.add(f64::from(t) / 1000.0);
            if let Some(seed) = m.seed {
                b.seed_pace[seed].add(f64::from(t) / 1000.0);
            }
        }
        if let Some(t) = s.nether {
            b.nether.add(f64::from(t) / 1000.0);
        }
    }
    b
}

/// Shared, source-fingerprinted measurements. The compact JSON cache and each
/// analysis's CSV follow --output into the containing directory. Old artifacts
/// are never deleted. Version changes invalidate cached measurement semantics.
pub(crate) fn load_research(input: &AnalysisInput<'_>) -> Result<ResearchData, Box<dyn Error>> {
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    "forfeit-research-v9-death-only-continuation".hash(&mut fingerprint);
    input.seasons().hash(&mut fingerprint);
    for file in input.files() {
        std::fs::canonicalize(&file.path)?.hash(&mut fingerprint);
        let meta = std::fs::metadata(&file.path)?;
        meta.len().hash(&mut fingerprint);
        meta.modified()?.hash(&mut fingerprint);
    }
    let cache = input.output().sibling(&format!(
        "forfeit_research_{:016x}.json.zst",
        fingerprint.finish()
    ));
    if cache.is_file() {
        return Ok(serde_json::from_reader(BufReader::new(
            zstd::stream::read::Decoder::new(File::open(cache)?)?,
        ))?);
    }
    let mut work = Vec::new();
    for file in input.files() {
        let reader = open(&file.path)?;
        let mut paths: Vec<Vec<&str>> = DUEL_COLUMNS
            .iter()
            .filter(|x| !x.ends_with("country") && !x.ends_with("nickname"))
            .map(|x| x.split('.').collect())
            .collect();
        paths.extend([
            vec!["type"],
            vec!["completions", "list", "element", "time"],
            vec!["completions", "list", "element", "uuid"],
        ]);
        let projection = project_schema(reader.metadata().file_metadata().schema(), &paths, 0)
            .ok_or_else(|| io::Error::other("no research columns"))?;
        for index in 0..reader.num_row_groups() {
            if row_group_matches(&reader, index, input.seasons()) {
                work.push((&file.path, index, projection.clone()));
            }
        }
    }
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build()?;
    let chunks: Result<Vec<_>, io::Error> = pool.install(|| {
        work.into_par_iter()
            .map(|(path, index, projection)| {
                let reader = open(path).map_err(|e| io::Error::other(e.to_string()))?;
                let rows = reader.get_row_group(index).map_err(io::Error::other)?;
                let mut matches = Vec::new();
                let mut audits: BTreeMap<u32, ResearchAudit> = BTreeMap::new();
                for row in rows
                    .get_row_iter(Some(projection))
                    .map_err(io::Error::other)?
                {
                    let row = row.map_err(io::Error::other)?;
                    let Some(season) =
                        research_int(&row, "season").and_then(|x| u32::try_from(x).ok())
                    else {
                        continue;
                    };
                    if !input.includes_season(season) {
                        continue;
                    }
                    let a = audits.entry(season).or_insert_with(|| ResearchAudit {
                        season,
                        ..ResearchAudit::default()
                    });
                    a.rows += 1;
                    if !matches!(research_field(&row, "decayed"), Some(Field::Bool(false))) {
                        continue;
                    }
                    if let Some(m) = research_match(&row, input.seasons()) {
                        matches.push(m);
                    } else {
                        a.skipped += 1;
                    }
                }
                Ok((matches, audits))
            })
            .collect()
    });
    let mut matches = Vec::new();
    let mut audits: BTreeMap<u32, ResearchAudit> = BTreeMap::new();
    for (chunk, counts) in chunks? {
        matches.extend(chunk);
        for (season, a) in counts {
            let target = audits.entry(season).or_insert_with(|| ResearchAudit {
                season,
                ..ResearchAudit::default()
            });
            target.rows += a.rows;
            target.skipped += a.skipped;
        }
    }
    matches.sort_unstable_by_key(|m| (m.season, m.id));
    let mut seen = HashSet::new();
    matches.retain(|m| {
        if seen.insert((m.season, m.id)) {
            true
        } else {
            audits.entry(m.season).or_default().duplicate_ids += 1;
            false
        }
    });
    drop(seen);
    matches.sort_unstable_by_key(|m| (m.season, m.date, m.id));
    let mut histories: BTreeMap<(u32, u128), Vec<(&ResearchMatch, usize)>> = BTreeMap::new();
    let mut player_cells: HashMap<(RecoveryKey, u128), RecoveryTally> = HashMap::new();
    let mut match_squares: HashMap<RecoveryKey, (f64, f64, f64, u64)> = HashMap::new();
    let mut calibration: BTreeMap<(u32, u32, bool), ResearchCalibration> = BTreeMap::new();
    let mut opponent_players: HashMap<(OpponentAfterKey, u128), OpponentAfterTally> =
        HashMap::new();
    let mut opponent_matches: HashMap<OpponentAfterKey, OpponentAfterMoments> = HashMap::new();
    for m in &matches {
        let a = audits.entry(m.season).or_default();
        a.played += 1;
        a.flagged += u64::from(m.flagged);
        a.draws += u64::from(m.winner.is_none());
        a.missing_timeline += u64::from(!m.timeline);
        a.missing_duration += u64::from(!m.duration_valid);
        a.invalid_events += m.invalid_events;
        a.excluded_death_resets += m.excluded_death_resets;
        a.post_forfeit_deaths += m.post_forfeit_deaths;
        a.explicit_quit += u64::from(m.sides.iter().any(|s| s.quit.is_some()));
        a.unknown_quit +=
            u64::from(m.flagged && m.winner.is_some() && m.sides.iter().all(|s| s.quit.is_none()));
        if a.start == 0 || m.date < a.start {
            a.start = m.date;
        }
        a.end = a.end.max(m.date);
        if let (Some(own), Some(other)) = (m.sides[0].elo, m.sides[1].elo) {
            let gap = (own - other).unsigned_abs();
            if gap < 500 {
                let favorite = usize::from(other > own);
                for season in [0, m.season] {
                    for all in [false, true] {
                        if !all && m.flagged {
                            continue;
                        }
                        let c = calibration.entry((season, gap / 50, all)).or_insert(
                            ResearchCalibration {
                                season,
                                gap: gap / 50,
                                all,
                                n: 0,
                                score: 0.0,
                                expected: 0.0,
                            },
                        );
                        c.n += 1;
                        c.score += m.winner.map_or(0.5, |w| f64::from(w == favorite));
                        c.expected += expected_score(own.max(other), own.min(other));
                    }
                }
            }
        }
        let mut in_match: HashMap<RecoveryKey, (f64, f64)> = HashMap::new();
        let mut opponent_in_match: HashMap<OpponentAfterKey, OpponentAfterTally> = HashMap::new();
        for (i, s) in m.sides.iter().enumerate() {
            histories
                .entry((m.season, s.uuid))
                .or_default()
                .push((m, i));
            if let Some(e) = s.events[0].filter(|e| e.time < m.opponent_event_end) {
                let other = &m.sides[1 - i];
                let later_death = other.last_death.is_some_and(|t| t > e.time);
                let later_forfeit = other
                    .quit
                    .is_some_and(|t| t > e.time && t <= m.opponent_event_end);
                let prior = u8::from(other.events[0].is_some_and(|d| d.time < e.time));
                let selected = [
                    true,
                    later_death,
                    later_forfeit,
                    !later_death && !later_forfeit,
                    later_death && !later_forfeit,
                    !later_death && later_forfeit,
                    later_death && later_forfeit,
                    !later_death,
                ];
                for rank in [research_rank(s.elo), 7] {
                    for behavior in [0, 1] {
                        if behavior == 1 && s.quit.is_some() {
                            continue;
                        }
                        for (prior_death, kind) in (0..8u8)
                            .map(|kind| (2, kind))
                            .chain([(prior, 1), (prior, 7)])
                        {
                            let key = OpponentAfterKey {
                                rank,
                                behavior,
                                prior_death,
                                kind,
                            };
                            let present = u64::from(selected[kind as usize]);
                            let t = OpponentAfterTally {
                                opportunities: 1,
                                selected: present,
                                wins: present * u64::from(m.winner == Some(i)),
                            };
                            opponent_players.entry((key, s.uuid)).or_default().merge(t);
                            opponent_in_match.entry(key).or_default().merge(t);
                        }
                    }
                }
            }
            for (kind, event) in s.events.iter().enumerate() {
                let Some(e) = event else {
                    continue;
                };
                let won = m.winner == Some(i);
                let own_quit = s.quit.is_some();
                for (season, rank, view, bin) in [
                    (0, research_rank(s.elo), 0, 0),
                    (0, 7, 0, 0),
                    (m.season, 7, 0, 0),
                    (0, 7, 1, e.stage as i8),
                    (0, 7, 2, e.deficit.clamp(-2, 2)),
                    (0, research_rank(s.elo), 2, e.deficit.clamp(-2, 2)),
                    (0, 7, 3, (e.time / 180_000).min(8) as i8),
                    (0, 7, 4, i8::from(e.opponent_adverse)),
                ] {
                    for behavior in 0..2 {
                        if behavior == 1 && own_quit {
                            continue;
                        }
                        let key = RecoveryKey {
                            season,
                            rank,
                            kind: kind as u8,
                            view,
                            bin,
                            behavior,
                        };
                        let t = player_cells.entry((key, s.uuid)).or_default();
                        t.n += 1;
                        t.wins += u64::from(won);
                        t.completion_wins += u64::from(won && s.completion.is_some());
                        if let Some(delta) = s.delta {
                            t.elo.add(f64::from(delta));
                        }
                        t.remaining_minutes += f64::from(m.duration - e.time) / 60_000.0;
                        let counts = in_match.entry(key).or_default();
                        counts.0 += 1.0;
                        counts.1 += f64::from(won);
                    }
                }
            }
        }
        for (key, (n, w)) in in_match {
            let sq = match_squares.entry(key).or_default();
            sq.0 += w * w;
            sq.1 += w * n;
            sq.2 += n * n;
            sq.3 += 1;
        }
        for (key, t) in opponent_in_match {
            opponent_matches.entry(key).or_default().add(t);
        }
    }
    let mut opponent_aggregate: HashMap<OpponentAfterKey, OpponentAfterAggregate> = HashMap::new();
    for ((key, _), t) in opponent_players {
        let a = opponent_aggregate.entry(key).or_default();
        a.tally.merge(t);
        a.moments.add(t);
    }
    let mut opponent_after = Vec::new();
    for (key, a) in opponent_aggregate {
        let m = &opponent_matches[&key];
        let t = a.tally;
        let (frequency_low, frequency_high) = binary_cluster_interval(
            t.selected,
            t.opportunities,
            a.moments.frequency,
            m.frequency,
        );
        let (win_low, win_high) =
            binary_cluster_interval(t.wins, t.selected, a.moments.winning, m.winning);
        opponent_after.push(OpponentAfterCell {
            key,
            opportunities: t.opportunities,
            selected: t.selected,
            wins: t.wins,
            opportunity_players: a.moments.frequency.clusters,
            selected_players: a.moments.winning.clusters,
            frequency_low,
            frequency_high,
            win_low,
            win_high,
        });
    }
    opponent_after
        .sort_unstable_by_key(|c| (c.key.rank, c.key.behavior, c.key.prior_death, c.key.kind));
    let mut aggregate: HashMap<RecoveryKey, (RecoveryTally, u64, f64, f64, f64, f64)> =
        HashMap::new();
    for ((key, _), t) in player_cells {
        let x = aggregate.entry(key).or_default();
        x.0.merge(&t);
        x.1 += 1;
        x.2 += (t.wins as f64).powi(2);
        x.3 += t.wins as f64 * t.n as f64;
        x.4 += (t.n as f64).powi(2);
        x.5 += t.wins as f64 / t.n as f64;
    }
    let mut recovery = Vec::new();
    for (key, (t, g, w2, wn, n2, means)) in aggregate {
        let n = t.n as f64;
        let p = t.wins as f64 / n;
        let player_var = (w2 - 2.0 * p * wn + p * p * n2).max(0.0) * g as f64
            / (g.saturating_sub(1).max(1) as f64);
        let (mw2, mwn, mn2, mg) = match_squares[&key];
        let match_var = (mw2 - 2.0 * p * mwn + p * p * mn2).max(0.0) * mg as f64
            / (mg.saturating_sub(1).max(1) as f64);
        let independent = n * p * (1.0 - p);
        // Conservative two-way player/match sandwich, never narrower than
        // either one-way cluster estimate or the independent Bernoulli floor.
        let variance = (player_var + match_var - independent)
            .max(player_var)
            .max(match_var)
            .max(independent)
            / (n * n);
        let half = 1.96 * variance.sqrt();
        let z2 = 1.96f64.powi(2);
        let center = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
        let wilson = 1.96 * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt() / (1.0 + z2 / n);
        recovery.push(RecoveryCell {
            key,
            n: t.n,
            wins: t.wins,
            completion_wins: t.completion_wins,
            players: g,
            player_mean: means / g as f64,
            low: (p - half).min(center - wilson).max(0.0),
            high: (p + half).max(center + wilson).min(1.0),
            elo: t.elo,
            remaining_minutes: t.remaining_minutes,
        });
    }
    recovery.sort_unstable_by_key(|x| {
        (
            x.key.season,
            x.key.rank,
            x.key.kind,
            x.key.view,
            x.key.bin,
            x.key.behavior,
        )
    });
    let mut profiles = Vec::new();
    let mut windows = Vec::new();
    let mut efficiency = Vec::new();
    for ((season, uuid), history) in histories {
        let rated: Vec<_> = history
            .iter()
            .copied()
            .filter(|(m, i)| m.sides[*i].elo.is_some() && m.sides[*i].delta.is_some())
            .collect();
        for pair in rated.windows(2) {
            let (a, i) = pair[0];
            let (b, j) = pair[1];
            let audit = audits.get_mut(&season).expect("season already audited");
            audit.continuity_checked += 1;
            audit.continuity_exact += u64::from(
                a.sides[i].elo.zip(a.sides[i].delta).map(|(x, y)| x + y) == b.sides[j].elo,
            );
        }
        if rated.is_empty() {
            continue;
        }
        let total = research_block(&rated);
        profiles.push(ResearchProfile {
            season,
            uuid: format!("{uuid:032x}"),
            all_games: history.len() as u64,
            rank: research_rank(total.elo.mean().map(|x| x as i32)),
            first: research_block(&rated[..rated.len().min(50)]),
            last: research_block(&rated[rated.len().saturating_sub(50)..]),
            total,
        });
        let audit = &audits[&season];
        for day in [30, 60, 90] {
            let anchor = audit.start + i64::from(day) * 86_400;
            // A full calendar follow-up must be present; partial season 12 is
            // intentionally excluded from the 30-day improvement cohorts.
            if anchor + 30 * 86_400 > audit.end {
                continue;
            }
            let split = rated.partition_point(|(m, _)| m.date < anchor);
            if split < 50 || rated[split - 1].0.date < anchor - 14 * 86_400 {
                continue;
            }
            let end = rated.partition_point(|(m, _)| m.date < anchor + 30 * 86_400);
            let before = research_block(&rated[split - 50..split]);
            let (last, side) = rated[split - 1];
            let rank = research_rank(
                last.sides[side]
                    .elo
                    .zip(last.sides[side].delta)
                    .map(|(x, y)| x + y),
            );
            windows.push(ResearchWindow {
                season,
                uuid: format!("{uuid:032x}"),
                day,
                rank,
                before,
                after: research_block(&rated[split..end]),
            });
        }
        let mut groups: BTreeMap<(u8, u8, i32), ResearchEfficiency> = BTreeMap::new();
        for (index, (m, i)) in rated.iter().enumerate() {
            if !m.duration_valid {
                continue;
            }
            let s = &m.sides[*i];
            let Some(e) = s.events[0] else {
                continue;
            };
            let action = u8::from(s.quit.is_none());
            let rank = research_rank(s.elo);
            let Some(other) = m.sides[1 - *i].elo else {
                continue;
            };
            let gap = ((s.elo.unwrap_or(0) - other).div_euclid(100)).clamp(-3, 3) + 3;
            let own_band = s.elo.unwrap_or(0).div_euclid(100);
            let stratum = own_band * 1000
                + gap * 100
                + i32::from(e.stage) * 10
                + i32::from(e.deficit.clamp(-2, 2) + 2);
            let group =
                groups
                    .entry((rank, action, stratum))
                    .or_insert_with(|| ResearchEfficiency {
                        season,
                        uuid: format!("{uuid:032x}"),
                        rank,
                        action,
                        stratum,
                        immediate: ResearchMoment::default(),
                        remaining_minutes: 0.0,
                        horizons: 0,
                        next_delta: 0.0,
                        next_minutes: 0.0,
                        session_horizons: 0,
                        session_delta: 0.0,
                        session_minutes: 0.0,
                    });
            group.immediate.add(f64::from(s.delta.unwrap_or(0)));
            group.remaining_minutes += f64::from(m.duration - e.time) / 60_000.0;
            if index + 10 <= rated.len()
                && rated[index..index + 10]
                    .iter()
                    .all(|(game, _)| game.duration_valid)
            {
                let next = research_block(&rated[index + 1..index + 10]);
                group.horizons += 1;
                group.next_delta += f64::from(s.delta.unwrap_or(0)) + next.delta.sum;
                group.next_minutes += f64::from(m.duration - e.time) / 60_000.0 + next.minutes;
                let slice = &rated[index..index + 10];
                let same_session = slice.windows(2).all(|p| {
                    p[1].0.date
                        <= p[0].0.date + i64::from(p[0].0.duration / 1000) + SESSION_GAP_SECONDS
                });
                if same_session {
                    group.session_horizons += 1;
                    group.session_delta += f64::from(s.delta.unwrap_or(0)) + next.delta.sum;
                    group.session_minutes +=
                        f64::from(m.duration - e.time) / 60_000.0 + next.minutes;
                }
            }
        }
        efficiency.extend(groups.into_values());
    }
    let data = ResearchData {
        audits: audits.into_values().collect(),
        recovery,
        profiles,
        windows,
        efficiency,
        calibration: calibration.into_values().collect(),
        opponent_after,
    };
    crate::plot::create_parent(&cache)?;
    // Write a new cache only after a complete successful computation.
    let temporary = cache.with_extension(format!("zst.{}.partial", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut writer = BufWriter::new(zstd::stream::write::Encoder::new(file, 1)?);
    serde_json::to_writer(&mut writer, &data)?;
    writer.flush()?;
    writer.into_inner().map_err(|e| e.into_error())?.finish()?;
    std::fs::rename(temporary, &cache)?;
    Ok(data)
}

/// Equal-weight player-season/cohort means with UUID-clustered uncertainty.
/// Shared by the habit, learning and retention plots. Samples are not treated
/// as independent merely because the same UUID appeared in another season.
#[derive(Clone)]
pub(crate) struct ResearchObservation {
    pub(crate) uuid: String,
    pub(crate) rank: u8,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) stratum: String,
}
#[derive(Clone, Copy)]
pub(crate) struct ResearchEstimate {
    pub(crate) x: f64,
    pub(crate) mean: f64,
    pub(crate) low: f64,
    pub(crate) high: f64,
    pub(crate) n: u64,
    pub(crate) players: u64,
}
pub(crate) fn research_estimates(
    samples: &[ResearchObservation],
    rank: u8,
) -> Vec<ResearchEstimate> {
    let mut bins: BTreeMap<u8, HashMap<&str, (f64, u64)>> = BTreeMap::new();
    for s in samples
        .iter()
        .filter(|s| s.rank == rank && s.x.is_finite() && s.y.is_finite())
    {
        let bin = ((s.x * 10.0).floor() as u8).min(9);
        let t = bins.entry(bin).or_default().entry(&s.uuid).or_default();
        t.0 += s.y;
        t.1 += 1;
    }
    bins.into_iter()
        .filter_map(|(bin, players)| {
            if players.len() < 20 {
                return None;
            }
            let n: u64 = players.values().map(|x| x.1).sum();
            let mean = players.values().map(|x| x.0).sum::<f64>() / n as f64;
            let g = players.len() as u64;
            let var = players
                .values()
                .map(|(sum, count)| (sum - mean * (*count as f64)).powi(2))
                .sum::<f64>()
                * g as f64
                / (g - 1) as f64
                / (n as f64).powi(2);
            let half = 1.96 * var.sqrt();
            Some(ResearchEstimate {
                x: (f64::from(bin) + 0.5) / 10.0,
                mean,
                low: mean - half,
                high: mean + half,
                n,
                players: g,
            })
        })
        .collect()
}
/// Removes predefined baseline-stratum means from the plotted outcome. The
/// regression itself demeans both exposure and outcome, with UUID clustering.
pub(crate) fn research_center(samples: &mut Vec<ResearchObservation>) -> Option<(f64, f64)> {
    let mut exposure_variation: HashMap<String, (u64, f64, f64)> = HashMap::new();
    for s in samples.iter() {
        let t = exposure_variation.entry(s.stratum.clone()).or_default();
        t.0 += 1;
        t.1 += s.x;
        t.2 += s.x * s.x;
    }
    samples.retain(|s| {
        let (n, sum, sq) = exposure_variation[&s.stratum];
        n >= 2 && sq - sum * sum / n as f64 > 1e-12
    });
    drop(exposure_variation);
    let mut strata: HashMap<&str, (f64, f64, u64)> = HashMap::new();
    for s in samples.iter() {
        let t = strata.entry(&s.stratum).or_default();
        t.0 += s.x;
        t.1 += s.y;
        t.2 += 1;
    }
    let mut residuals = Vec::with_capacity(samples.len());
    let mut xx = 0.0;
    let mut xy = 0.0;
    for s in samples.iter() {
        let (x, y, n) = strata[&s.stratum.as_str()];
        let dx = s.x - x / n as f64;
        let dy = s.y - y / n as f64;
        xx += dx * dx;
        xy += dx * dy;
        residuals.push((dx, dy));
    }
    drop(strata);
    if xx <= 1e-12 {
        return None;
    }
    let slope = xy / xx;
    let mut scores: HashMap<&str, f64> = HashMap::new();
    for (s, (x, y)) in samples.iter().zip(&residuals) {
        *scores.entry(&s.uuid).or_default() += x * (y - slope * x);
    }
    let g = scores.len();
    let se = (scores.values().map(|x| x * x).sum::<f64>() * (g as f64)
        / (g.saturating_sub(1).max(1) as f64)
        / (xx * xx))
        .sqrt();
    drop(scores);
    for (s, (_, y)) in samples.iter_mut().zip(residuals) {
        s.y = y;
    }
    Some((slope, se))
}
pub(crate) fn research_csv(
    path: &Path,
    header: &str,
    rows: impl IntoIterator<Item = String>,
) -> io::Result<()> {
    crate::plot::create_parent(path)?;
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "{header}")?;
    for row in rows {
        writeln!(writer, "{row}")?;
    }
    writer.flush()
}
