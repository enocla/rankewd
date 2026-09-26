use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::file::statistics::Statistics;
use parquet::record::{Field, List, Row};
use parquet::schema::parser::parse_message_type;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io;
use std::path::Path;

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
    let projection = parse_message_type(RECORD_FILTER_PROJECTION)?;
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
        let projection = parse_message_type(if keep_history {
            PLAYER_PROJECTION
        } else {
            PLAYER_SUMMARY_PROJECTION
        })?;

        for path in paths {
            let reader = open(path)?;
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
    let projection = parse_message_type(HOUR_OF_DAY_PROJECTION)?;

    for path in paths {
        let reader = open(path)?;
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
