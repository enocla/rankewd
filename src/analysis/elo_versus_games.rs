use super::{Analysis, AnalysisInput, AnalysisReport, DatasetFile};
use crate::dataset;
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, CYAN_600, FONT_FAMILY, GREEN_600, LABEL_AREA_SIZE,
    LEGEND_FONT_SIZE, LEGEND_WIDTH, PAPER, RED_600, TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE,
    YELLOW_600, create_parent,
};
use arrow_array::{ArrayRef, Float64Array, Int64Array, StringArray, UInt32Array};
use arrow_schema::{DataType, Field as ArrowField, Schema};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::file::reader::FileReader;
use parquet::record::Field;
use parquet::schema::parser::parse_message_type;
use plotters::coord::types::{RangedCoordf64, RangedCoordi64};
use plotters::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

const PROJECTION: &str = "message root { \
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
}";
const PLAYER_DATA_FILE: &str = "player_elos.parquet";
const MIN_GAMES_FOR_WINRATE_PLOTS: u64 = 100;
const POINT_RADIUS: i32 = 2;
const ELO_BIN_SIZE: i64 = 50;
const WINRATE_BIN_SIZE: i64 = 100;
const MIN_WINRATE_BIN_PLAYERS: u64 = 20;

pub(super) struct EloVersusGames;

impl Analysis for EloVersusGames {
    fn name(&self) -> &'static str {
        "elo-versus-games"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let mut data = EloData::from_files(input.files(), input.seasons())?;
        if data.players.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no player records found in selected files",
            )
            .into());
        }

        let mut by_season = BTreeMap::<u32, Vec<&PlayerSeason>>::new();
        for player in &data.players {
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

        let data_path = input.output().sibling(PLAYER_DATA_FILE);
        write_player_parquet(&data_path, &data.players)?;

        let mut report = AnalysisReport::default();
        let mut chart_count = 0;
        for (season, season_players) in by_season {
            let suffix = format!("_{season}");
            let scatter_path = input
                .output()
                .with_stem("elo_vs_games_season", "png", &suffix);
            let distribution_path = input
                .output()
                .sibling(&format!("elo_distribution_season_{season}.png"));
            let winrate_path = input
                .output()
                .sibling(&format!("winrate_vs_elo_season_{season}.png"));
            let history_path = input
                .output()
                .sibling(&format!("winrate_over_time_season_{season}.png"));

            draw_elo_vs_games(&scatter_path, season, &season_players)?;
            draw_elo_distribution(&distribution_path, season, &season_players)?;
            let season_history = data.history_by_season.remove(&season).ok_or_else(|| {
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
                }
            }

            report.line(format!(
                "Season {season}: {} rated players; {} with at least {MIN_GAMES_FOR_WINRATE_PLOTS} matches",
                season_players.len(),
                winrate_players.len()
            ));
            report.line(format!(
                "  Elo charts: {}, {}",
                scatter_path.display(),
                distribution_path.display()
            ));
            if !winrate_players.is_empty() {
                report.line(format!("  Win rate vs Elo: {}", winrate_path.display()));
            }
            if temporal_chart_written {
                report.line(format!("  Win rate over time: {}", history_path.display()));
            }
        }

        report.line(format!(
            "Wrote {} player-season rows to {}",
            data.players.len(),
            data_path.display()
        ));
        report.line(format!("Wrote {chart_count} charts"));
        Ok(report)
    }
}

struct EloData {
    players: Vec<PlayerSeason>,
    history_by_season: BTreeMap<u32, SeasonHistory>,
}

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

#[derive(Clone, Copy, Debug)]
struct TemporalPoint {
    elo: i64,
    win_rate: f64,
    progress: f64,
}

impl EloData {
    fn from_files(files: &[DatasetFile], seasons: Option<&[u32]>) -> Result<Self, Box<dyn Error>> {
        let mut players = BTreeMap::<(u32, String), PlayerSeason>::new();
        let mut history_by_season = BTreeMap::<u32, SeasonHistory>::new();

        for file in files {
            let reader = dataset::open(&file.path)?;
            let projection = parse_message_type(PROJECTION)?;

            for row in reader.get_row_iter(Some(projection.clone()))? {
                let row = row?;
                let season = dataset::integer_field(&row, "season")?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing season"))?;
                let season = u32::try_from(season).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
                })?;
                if seasons.is_some_and(|requested| !requested.contains(&season)) {
                    continue;
                }

                let date = dataset::integer_field(&row, "date")?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "missing match date")
                })?;
                let match_id = dataset::integer_field(&row, "id")?.unwrap_or_default();
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

                let Some(participants) = dataset::list_field(&row, "players")? else {
                    continue;
                };
                let winner_uuid = dataset::group_field(&row, "result")?
                    .map(|result| dataset::string_field(result, "uuid"))
                    .transpose()?
                    .flatten();
                let mut match_participants = Vec::new();
                let mut seen_in_match = HashSet::new();

                for participant in participants.elements() {
                    let Field::Group(participant) = participant else {
                        return Err(dataset::invalid_field("player entry", participant).into());
                    };
                    let Some(uuid) = dataset::string_field(participant, "uuid")? else {
                        continue;
                    };
                    if !seen_in_match.insert(uuid.to_owned()) {
                        continue;
                    }

                    match_participants.push(Participant {
                        uuid: uuid.to_owned(),
                        nickname: dataset::string_field(participant, "nickname")?
                            .unwrap_or_default()
                            .to_owned(),
                        elo: dataset::integer_field(participant, "eloRate")?,
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

        Ok(Self {
            players: players.into_values().collect(),
            history_by_season,
        })
    }
}

impl SeasonHistory {
    fn into_points(
        self,
        eligible_uuids: &HashSet<&str>,
    ) -> Result<Vec<TemporalPoint>, Box<dyn Error>> {
        let first_date = self.first_date.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "season history has no dates")
        })?;
        let last_date = self.last_date.unwrap_or(first_date);
        let duration = last_date.saturating_sub(first_date).max(1) as f64;
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
                if let Some(elo) = event.elo
                    && decided_matches > 0
                {
                    let win_rate = wins as f64 / decided_matches as f64;
                    let progress =
                        (event.date.saturating_sub(first_date) as f64 / duration).clamp(0.0, 1.0);
                    points.push(TemporalPoint {
                        elo,
                        win_rate,
                        progress,
                    });
                }
            }
        }

        Ok(points)
    }
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

fn draw_elo_vs_games(
    path: &Path,
    season: u32,
    players: &[&PlayerSeason],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let points: Vec<_> = players
        .iter()
        .filter_map(|player| Some((i64::try_from(player.games_played).ok()?, player.latest_elo?)))
        .collect();
    let max_games = points
        .iter()
        .map(|(games, _)| *games)
        .max()
        .unwrap_or(1)
        .max(1);
    let (min_elo, max_elo) = elo_bounds(&points)?;
    let (min_elo, max_elo) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Latest Elo vs Games Played"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0i64..(max_games + max_games / 20 + 1), min_elo..max_elo)?;

    chart
        .configure_mesh()
        .x_desc("Games played in season")
        .y_desc("Latest Elo in season")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(8)
        .y_labels(8)
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(points.iter().map(|(games, elo)| {
            Circle::new((*games, *elo), POINT_RADIUS, BLUE_600.mix(0.35).filled())
        }))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    draw_legend(&mut chart)?;
    root.present()?;
    Ok(())
}

fn draw_elo_distribution(
    path: &Path,
    season: u32,
    players: &[&PlayerSeason],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let mut bins = BTreeMap::<i64, u64>::new();
    for elo in players.iter().filter_map(|player| player.latest_elo) {
        *bins
            .entry(elo.div_euclid(ELO_BIN_SIZE) * ELO_BIN_SIZE)
            .or_default() += 1;
    }
    if bins.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot").into());
    }
    let first_bin = *bins.first_key_value().expect("non-empty bins").0;
    let last_bin = *bins.last_key_value().expect("non-empty bins").0;
    let max_count = *bins.values().max().expect("non-empty bins");
    let x_end = last_bin.saturating_add(ELO_BIN_SIZE);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Elo Distribution"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(first_bin..x_end, 0u64..(max_count + max_count / 10 + 1))?;

    chart
        .configure_mesh()
        .x_desc("Latest Elo in season (50 point bins)")
        .y_desc("Players")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(8)
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(bins.iter().map(|(bin, count)| {
            Rectangle::new(
                [(*bin, 0), (bin.saturating_add(ELO_BIN_SIZE - 4), *count)],
                BLUE_600.mix(0.85).filled(),
            )
        }))?
        .label("Players")
        .legend(|(x, y)| {
            Rectangle::new(
                [(x, y - 5), (x + LEGEND_WIDTH, y + 5)],
                BLUE_600.mix(0.85).filled(),
            )
        });
    draw_legend(&mut chart)?;
    root.present()?;
    Ok(())
}

fn draw_winrate_vs_elo(
    path: &Path,
    season: u32,
    players: &[&PlayerSeason],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let points: Vec<_> = players
        .iter()
        .filter_map(|player| Some((player.latest_elo?, player.win_rate()?)))
        .collect();
    if points.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no players with Elo and decided matches to plot",
        )
        .into());
    }

    let min_elo = points.iter().map(|(elo, _)| *elo).min().expect("non-empty");
    let max_elo = points.iter().map(|(elo, _)| *elo).max().expect("non-empty");
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);
    let mut bins = BTreeMap::<i64, (f64, u64)>::new();
    for (elo, win_rate) in &points {
        let bin = elo.div_euclid(WINRATE_BIN_SIZE) * WINRATE_BIN_SIZE;
        let (sum, count) = bins.entry(bin).or_default();
        *sum += win_rate;
        *count += 1;
    }
    let mean_by_elo: Vec<_> = bins
        .into_iter()
        .filter_map(|(bin, (sum, count))| {
            (count >= MIN_WINRATE_BIN_PLAYERS)
                .then_some((bin + WINRATE_BIN_SIZE / 2, sum / count as f64))
        })
        .collect();

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Player Winrate vs Elo"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(x_min..x_max, 0.0f64..1.0f64)?;

    chart
        .configure_mesh()
        .x_desc("Latest Elo in season")
        .y_desc("Win rate (decided matches)")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(6)
        .y_label_formatter(&|rate| format!("{:.0}%", rate * 100.0))
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart.draw_series(LineSeries::new(
        [(x_min, 0.5), (x_max, 0.5)],
        BASE_300.stroke_width(2),
    ))?;
    chart
        .draw_series(points.iter().map(|(elo, win_rate)| {
            Circle::new((*elo, *win_rate), POINT_RADIUS, BLUE_600.mix(0.25).filled())
        }))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    chart
        .draw_series(LineSeries::new(
            mean_by_elo.iter().copied(),
            GREEN_600.stroke_width(3),
        ))?
        .label("Mean by Elo (20+ players per bin)")
        .legend(|(x, y)| {
            PathElement::new(
                vec![(x, y), (x + LEGEND_WIDTH, y)],
                GREEN_600.stroke_width(3),
            )
        });
    chart.draw_series(
        mean_by_elo
            .iter()
            .map(|(elo, win_rate)| Circle::new((*elo, *win_rate), 4, GREEN_600.filled())),
    )?;
    draw_legend(&mut chart)?;
    root.present()?;
    Ok(())
}

fn draw_winrate_over_time(
    path: &Path,
    season: u32,
    points: &[TemporalPoint],
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let min_elo = points
        .iter()
        .map(|point| point.elo)
        .min()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no temporal Elo values"))?;
    let max_elo = points
        .iter()
        .map(|point| point.elo)
        .max()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no temporal Elo values"))?;
    let (x_min, x_max) = padded_bounds(min_elo, max_elo);

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Winrate vs Elo Over Time"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(x_min..x_max, 0.0f64..1.0f64)?;

    chart
        .configure_mesh()
        .x_desc("Elo at match time")
        .y_desc("Cumulative win rate at match time")
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(10)
        .y_labels(6)
        .y_label_formatter(&|rate| format!("{:.0}%", rate * 100.0))
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart.draw_series(points.iter().map(|point| {
        Circle::new(
            (point.elo, point.win_rate),
            POINT_RADIUS,
            progress_color(point.progress).mix(0.50).filled(),
        )
    }))?;
    draw_progress_legend(&chart, x_min, x_max)?;
    root.present()?;
    Ok(())
}

fn draw_legend<'a, DB, X, Y>(
    chart: &mut ChartContext<'a, DB, Cartesian2d<X, Y>>,
) -> Result<(), Box<dyn Error>>
where
    DB: DrawingBackend + 'a,
    DB::ErrorType: 'static,
    X: Ranged,
    Y: Ranged,
    Cartesian2d<X, Y>: CoordTranslate,
{
    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE, &BASE_800))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    Ok(())
}

fn draw_progress_legend<DB: DrawingBackend>(
    chart: &ChartContext<'_, DB, Cartesian2d<RangedCoordi64, RangedCoordf64>>,
    x_min: i64,
    x_max: i64,
) -> Result<(), Box<dyn Error>>
where
    DB::ErrorType: 'static,
{
    let span = x_max.saturating_sub(x_min).max(1);
    let margin = (span / 50).max(1);
    let right = x_max.saturating_sub(margin);
    let left = right.saturating_sub((span / 3).max(1)).max(x_min);
    let bottom = 0.895;
    let top = 0.995;
    let bar_bottom = 0.935;
    let bar_top = 0.96;
    let area = chart.plotting_area();

    area.draw(&Rectangle::new(
        [(left, bottom), (right, top)],
        PAPER.filled().stroke_width(1),
    ))?;
    area.draw(&Text::new(
        "Season progress",
        (left + margin, 0.979),
        (FONT_FAMILY, 18).into_font().color(&BASE_800),
    ))?;

    const SEGMENTS: i64 = 160;
    for segment in 0..SEGMENTS {
        let segment_left = left + (right - left) * segment / SEGMENTS;
        let segment_right = left + (right - left) * (segment + 1) / SEGMENTS;
        area.draw(&Rectangle::new(
            [(segment_left, bar_bottom), (segment_right, bar_top)],
            progress_color(segment as f64 / (SEGMENTS - 1) as f64).filled(),
        ))?;
    }
    area.draw(&Rectangle::new(
        [(left, bar_bottom), (right, bar_top)],
        BASE_800.stroke_width(1),
    ))?;
    area.draw(&Text::new(
        "Start",
        (left + margin, 0.91),
        (FONT_FAMILY, 16).into_font().color(&BASE_800),
    ))?;
    area.draw(&Text::new(
        "End",
        (right.saturating_sub(margin * 5), 0.91),
        (FONT_FAMILY, 16).into_font().color(&BASE_800),
    ))?;
    Ok(())
}

fn progress_color(progress: f64) -> RGBColor {
    const STOPS: [RGBColor; 5] = [BLUE_600, CYAN_600, GREEN_600, YELLOW_600, RED_600];
    let position = progress.clamp(0.0, 1.0) * (STOPS.len() - 1) as f64;
    let start_index = (position.floor() as usize).min(STOPS.len() - 2);
    let fraction = position - start_index as f64;
    let start = STOPS[start_index];
    let end = STOPS[start_index + 1];
    let interpolate = |left: u8, right: u8| {
        (f64::from(left) + (f64::from(right) - f64::from(left)) * fraction).round() as u8
    };
    RGBColor(
        interpolate(start.0, end.0),
        interpolate(start.1, end.1),
        interpolate(start.2, end.2),
    )
}

fn elo_bounds(points: &[(i64, i64)]) -> Result<(i64, i64), Box<dyn Error>> {
    let min = points
        .iter()
        .map(|(_, elo)| *elo)
        .min()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot"))?;
    let max = points
        .iter()
        .map(|(_, elo)| *elo)
        .max()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no Elo values to plot"))?;
    Ok((min, max))
}

fn padded_bounds(min: i64, max: i64) -> (i64, i64) {
    let span = max.saturating_sub(min).max(1);
    let padding = (span / 20).max(25);
    (min.saturating_sub(padding), max.saturating_add(padding))
}
