use super::{Analysis, DatasetFile};
use arrow_array::{ArrayRef, Float64Array, Int64Array, StringArray, UInt32Array};
use arrow_schema::{DataType, Field as ArrowField, Schema};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, List, Row};
use parquet::schema::parser::parse_message_type;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod style;

use self::style::{
    draw_elo_distribution, draw_elo_vs_games, draw_winrate_over_time, draw_winrate_vs_elo,
};

const PLAYER_DATA_FILE: &str = "dataset/player_elos.parquet";
const MIN_GAMES_FOR_WINRATE_PLOTS: u64 = 100;

/// Plots player Elo against games played and Elo distributions, separately by season.
pub struct EloVersusGames;

#[derive(Clone, Debug)]
struct PlayerSeason {
    season: u32,
    uuid: String,
    nickname: String,
    games_played: u64,
    wins: u64,
    decided_matches: u64,
    latest_elo: Option<i64>,
    latest_elo_date: Option<i64>,
    last_seen_date: Option<i64>,
}

impl PlayerSeason {
    fn win_rate(&self) -> Option<f64> {
        (self.decided_matches > 0).then(|| self.wins as f64 / self.decided_matches as f64)
    }
}

struct Participant {
    uuid: String,
    nickname: String,
    elo: Option<i64>,
}

#[derive(Default)]
struct SeasonHistory {
    players: HashMap<String, Vec<TemporalEvent>>,
    first_date: Option<i64>,
    last_date: Option<i64>,
}

struct TemporalEvent {
    date: i64,
    match_id: i64,
    elo: Option<i64>,
    won: bool,
    decided: bool,
}

impl Analysis for EloVersusGames {
    fn name(&self) -> &'static str {
        "elo-versus-games"
    }

    fn run(&self, files: &[DatasetFile], output: Option<&Path>) -> Result<(), Box<dyn Error>> {
        let mut players = BTreeMap::<(u32, String), PlayerSeason>::new();
        let mut history_by_season = BTreeMap::<u32, SeasonHistory>::new();

        for dataset_file in files {
            let reader = SerializedFileReader::new(File::open(&dataset_file.path)?)?;
            let projection = parse_message_type(
                "message root { \
                    optional int64 id; \
                    optional int64 date; \
                    optional int64 season; \
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
                }",
            )?;

            for row in reader.get_row_iter(Some(projection.clone()))? {
                let row = row?;
                let date = integer_field(&row, "date")?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing match date")
                })?;
                let season = integer_field(&row, "season")?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing season"))?;
                let season = u32::try_from(season).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
                })?;
                let match_id = integer_field(&row, "id")?.unwrap_or_default();
                let season_history = history_by_season.entry(season).or_default();
                season_history.first_date = Some(
                    season_history
                        .first_date
                        .map_or(date, |first_date| first_date.min(date)),
                );
                season_history.last_date = Some(
                    season_history
                        .last_date
                        .map_or(date, |last_date| last_date.max(date)),
                );
                let participants = list_field(&row, "players")?;
                let Some(participants) = participants else {
                    continue;
                };
                let winner_uuid = group_field(&row, "result")?
                    .map(|result| string_field(result, "uuid"))
                    .transpose()?
                    .flatten();
                let mut match_participants = Vec::new();
                let mut seen_in_match = HashSet::new();

                for participant in participants.elements() {
                    let Field::Group(participant) = participant else {
                        return Err(invalid_field("player entry", participant).into());
                    };
                    let Some(uuid) = string_field(participant, "uuid")? else {
                        continue;
                    };
                    if !seen_in_match.insert(uuid.to_owned()) {
                        continue;
                    }

                    match_participants.push(Participant {
                        uuid: uuid.to_owned(),
                        nickname: string_field(participant, "nickname")?
                            .unwrap_or_default()
                            .to_owned(),
                        elo: integer_field(participant, "eloRate")?,
                    });
                }
                let has_winner = winner_uuid.as_ref().is_some_and(|winner| {
                    match_participants
                        .iter()
                        .any(|participant| &participant.uuid == winner)
                });

                for participant in match_participants {
                    let won = winner_uuid.as_deref() == Some(participant.uuid.as_str());
                    let player = players
                        .entry((season, participant.uuid.clone()))
                        .or_insert_with(|| PlayerSeason {
                            season,
                            uuid: participant.uuid.clone(),
                            nickname: participant.nickname.clone(),
                            games_played: 0,
                            wins: 0,
                            decided_matches: 0,
                            latest_elo: None,
                            latest_elo_date: None,
                            last_seen_date: None,
                        });
                    player.games_played = player.games_played.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "games played overflow")
                    })?;
                    if player
                        .last_seen_date
                        .is_none_or(|seen_date| date >= seen_date)
                    {
                        player.nickname = participant.nickname;
                        player.last_seen_date = Some(date);
                    }
                    if has_winner {
                        player.decided_matches =
                            player.decided_matches.checked_add(1).ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "decided match count overflow",
                                )
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
                    }
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
                        });
                }
            }
        }

        if players.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no player records found in selected files",
            )
            .into());
        }

        let player_rows: Vec<_> = players.into_values().collect();
        let data_path = Path::new(PLAYER_DATA_FILE);
        write_player_parquet(data_path, &player_rows)?;

        let mut by_season = BTreeMap::<u32, Vec<&PlayerSeason>>::new();
        for player in &player_rows {
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

        let mut chart_count = 0;
        for (season, season_players) in by_season {
            let (scatter_path, distribution_path) = season_output_paths(output, season);
            let winrate_path = winrate_output_path(output, season);
            let history_path = winrate_progress_output_path(output, season);
            draw_elo_vs_games(&scatter_path, season, &season_players)?;
            draw_elo_distribution(&distribution_path, season, &season_players)?;
            let season_history = history_by_season.remove(&season).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing season match history")
            })?;
            chart_count += 2;

            let winrate_players: Vec<_> = season_players
                .iter()
                .copied()
                .filter(|player| {
                    player.games_played >= MIN_GAMES_FOR_WINRATE_PLOTS
                        && player.win_rate().is_some()
                })
                .collect();
            let mut temporal_chart_written = false;
            if !winrate_players.is_empty() {
                draw_winrate_vs_elo(&winrate_path, season, &winrate_players)?;
                chart_count += 1;
                let eligible_uuids: HashSet<&str> = winrate_players
                    .iter()
                    .map(|player| player.uuid.as_str())
                    .collect();
                let temporal_points = season_history.into_points(&eligible_uuids)?;
                if !temporal_points.is_empty() {
                    draw_winrate_over_time(&history_path, season, &temporal_points)?;
                    temporal_chart_written = true;
                    chart_count += 1;
                } else {
                    remove_stale_chart(&history_path)?;
                }
            } else {
                remove_stale_chart(&winrate_path)?;
                remove_stale_chart(&history_path)?;
            }
            println!(
                "Season {season}: {} rated players; {} with at least {MIN_GAMES_FOR_WINRATE_PLOTS} matches",
                season_players.len(),
                winrate_players.len()
            );
            println!(
                "  Elo charts: {}, {}",
                scatter_path.display(),
                distribution_path.display()
            );
            if !winrate_players.is_empty() {
                println!("  Win rate vs Elo: {}", winrate_path.display());
            }
            if temporal_chart_written {
                println!("  Win rate over time: {}", history_path.display());
            }
        }

        println!(
            "Wrote {} player-season rows to {}",
            player_rows.len(),
            data_path.display()
        );
        println!("Wrote {chart_count} charts");
        Ok(())
    }
}

fn integer_field(row: &Row, name: &str) -> Result<Option<i64>, Box<dyn Error>> {
    let value = row
        .get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value));
    match value {
        Some(Field::Long(value)) => Ok(Some(*value)),
        Some(Field::Int(value)) => Ok(Some(i64::from(*value))),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => {
            Err(io::Error::new(io::ErrorKind::InvalidData, format!("missing {name} column")).into())
        }
    }
}

fn string_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a str>, Box<dyn Error>> {
    let value = row
        .get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value));
    match value {
        Some(Field::Str(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => {
            Err(io::Error::new(io::ErrorKind::InvalidData, format!("missing {name} column")).into())
        }
    }
}

fn list_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a List>, Box<dyn Error>> {
    let value = row
        .get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value));
    match value {
        Some(Field::ListInternal(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => {
            Err(io::Error::new(io::ErrorKind::InvalidData, format!("missing {name} column")).into())
        }
    }
}

fn invalid_field(name: &str, value: &Field) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unexpected value for {name}: {value:?}"),
    )
}

fn write_player_parquet(path: &Path, players: &[PlayerSeason]) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let schema = Arc::new(Schema::new(vec![
        ArrowField::new("season", DataType::UInt32, false),
        ArrowField::new("uuid", DataType::Utf8, false),
        ArrowField::new("nickname", DataType::Utf8, false),
        ArrowField::new("games_played", DataType::Int64, false),
        ArrowField::new("wins", DataType::Int64, false),
        ArrowField::new("decided_matches", DataType::Int64, false),
        ArrowField::new("win_rate", DataType::Float64, true),
        ArrowField::new("latest_elo", DataType::Int64, true),
        ArrowField::new("latest_elo_date", DataType::Int64, true),
    ]));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(UInt32Array::from(
            players
                .iter()
                .map(|player| player.season)
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from_iter_values(
            players.iter().map(|player| player.uuid.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            players.iter().map(|player| player.nickname.as_str()),
        )),
        Arc::new(Int64Array::from(
            players
                .iter()
                .map(|player| i64::try_from(player.games_played))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Arc::new(Int64Array::from(
            players
                .iter()
                .map(|player| i64::try_from(player.wins))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Arc::new(Int64Array::from(
            players
                .iter()
                .map(|player| i64::try_from(player.decided_matches))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Arc::new(Float64Array::from(
            players
                .iter()
                .map(|player| player.win_rate())
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            players
                .iter()
                .map(|player| player.latest_elo)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            players
                .iter()
                .map(|player| player.latest_elo_date)
                .collect::<Vec<_>>(),
        )),
    ];
    let batch = arrow_array::RecordBatch::try_new(schema.clone(), columns)?;
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

fn season_output_paths(output: Option<&Path>, season: u32) -> (PathBuf, PathBuf) {
    match output {
        None => (
            PathBuf::from(format!("elo_vs_games_season_{season}.png")),
            PathBuf::from(format!("elo_distribution_season_{season}.png")),
        ),
        Some(path) => {
            let parent = path.parent().unwrap_or_else(|| Path::new(""));
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("elo_vs_games");
            (
                parent.join(format!("{stem}_season_{season}.png")),
                parent.join(format!("elo_distribution_season_{season}.png")),
            )
        }
    }
}

fn winrate_output_path(output: Option<&Path>, season: u32) -> PathBuf {
    match output {
        None => PathBuf::from(format!("winrate_vs_elo_season_{season}.png")),
        Some(path) => path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(format!("winrate_vs_elo_season_{season}.png")),
    }
}

fn winrate_progress_output_path(output: Option<&Path>, season: u32) -> PathBuf {
    match output {
        None => PathBuf::from(format!("winrate_over_time_season_{season}.png")),
        Some(path) => path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(format!("winrate_over_time_season_{season}.png")),
    }
}

impl SeasonHistory {
    fn into_points(
        self,
        eligible_uuids: &HashSet<&str>,
    ) -> Result<Vec<(i64, f64, f64)>, Box<dyn Error>> {
        let first_date = self.first_date.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "season history has no dates")
        })?;
        let last_date = self.last_date.unwrap_or(first_date);
        let duration = (last_date - first_date).max(1) as f64;
        let mut points = Vec::new();

        for (uuid, mut events) in self.players {
            if !eligible_uuids.contains(uuid.as_str()) {
                continue;
            }
            events.sort_unstable_by_key(|event| (event.date, event.match_id));
            let mut wins = 0u64;
            let mut decided_matches = 0u64;
            for event in events {
                if event.decided {
                    decided_matches += 1;
                    if event.won {
                        wins += 1;
                    }
                }
                if let (Some(elo), true) = (event.elo, decided_matches > 0) {
                    let win_rate = wins as f64 / decided_matches as f64;
                    let progress = ((event.date - first_date) as f64 / duration).clamp(0.0, 1.0);
                    points.push((elo, win_rate, progress));
                }
            }
        }

        Ok(points)
    }
}

fn group_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a Row>, Box<dyn Error>> {
    let value = row
        .get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value));
    match value {
        Some(Field::Group(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => {
            Err(io::Error::new(io::ErrorKind::InvalidData, format!("missing {name} column")).into())
        }
    }
}

fn create_parent(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn remove_stale_chart(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
