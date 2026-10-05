use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_500, BASE_800, Line, LineChart, SERIES_COLORS, draw_line_chart, integer_tick, value_range,
};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;

const OUTPUT_FILE_PREFIX: &str = "top_ten_trajectories_season_";
const TOP_PLAYERS: usize = 10;
/// Rated matches a player needs to qualify for the top ten.
const MIN_RATED_MATCHES: u32 = 30;
const SECONDS_PER_DAY: i64 = 86_400;

pub(super) struct TopTenTrajectories;

impl Analysis for TopTenTrajectories {
    fn name(&self) -> &'static str {
        "top-ten-trajectories"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns {
                nicknames: true,
                ..DuelColumns::default()
            },
        )?;
        let ratings = duels.ratings();
        let mut report = AnalysisReport::default();
        for season in duels.seasons() {
            let mut top: Vec<(u128, i32)> = ratings
                .iter()
                .filter(|((rated_season, _), rating)| {
                    *rated_season == season && rating.rated_matches >= MIN_RATED_MATCHES
                })
                .map(|((_, uuid), rating)| (*uuid, rating.final_elo))
                .collect();
            top.sort_unstable_by_key(|(uuid, elo)| (std::cmp::Reverse(*elo), *uuid));
            top.truncate(TOP_PLAYERS);
            if top.is_empty() {
                continue;
            }

            let season_start = duels
                .duels
                .iter()
                .find(|duel| duel.season == season)
                .map_or(0, |duel| duel.date);
            let rank: HashMap<u128, usize> = top
                .iter()
                .enumerate()
                .map(|(index, (uuid, _))| (*uuid, index))
                .collect();
            // Daily closing Elo: the rating after each player's last match of a day.
            let mut trajectories = vec![BTreeMap::<i64, f64>::new(); top.len()];
            for duel in duels.duels.iter().filter(|duel| duel.season == season) {
                for side in 0..2 {
                    if let (Some(index), Some(after)) =
                        (rank.get(&duel.sides[side].uuid), duel.elo_after(side))
                    {
                        let day = (duel.date - season_start).div_euclid(SECONDS_PER_DAY);
                        trajectories[*index].insert(day, f64::from(after));
                    }
                }
            }

            let colors: Vec<_> = SERIES_COLORS
                .iter()
                .copied()
                .chain([BASE_800, BASE_500])
                .collect();
            let lines: Vec<Line> = top
                .iter()
                .zip(trajectories)
                .enumerate()
                .map(|(index, ((uuid, elo), points))| {
                    let name = duels
                        .nicknames
                        .get(&(season, *uuid))
                        .cloned()
                        .unwrap_or_else(|| format!("{uuid:032x}")[..8].to_owned());
                    let mut line = Line::new(
                        format!("#{} {name} ({elo})", index + 1),
                        colors[index % colors.len()],
                        points
                            .into_iter()
                            .map(|(day, elo)| ((day + 1) as f64, elo))
                            .collect(),
                    );
                    line.markers = false;
                    line
                })
                .collect();
            let x_range = value_range(
                lines
                    .iter()
                    .flat_map(|line| line.points.iter().map(|p| p.0)),
                0.0,
                0.0,
            );
            let y_range = value_range(
                lines
                    .iter()
                    .flat_map(|line| line.points.iter().map(|p| p.1)),
                0.03,
                0.95,
            );
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{season}.png"));
            draw_line_chart(
                &path,
                &LineChart {
                    title: format!("Season {season}: How the Final Top Ten Got There"),
                    x_desc: "Days into season",
                    y_desc: "Daily closing Elo",
                    x_range: 0.0..x_range.end.max(1.0),
                    y_range,
                    x_labels: 10,
                    x_format: &integer_tick,
                    y_format: &integer_tick,
                    lines,
                    reference: None,
                },
            )?;
            report.line(format!(
                "Season {season}: top {} final Elo {}",
                top.len(),
                top.iter()
                    .map(|(uuid, elo)| format!(
                        "{} {elo}",
                        duels
                            .nicknames
                            .get(&(season, *uuid))
                            .map_or("?", String::as_str)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            report.line(format!("  Top ten trajectories: {}", path.display()));
        }
        Ok(report)
    }
}
