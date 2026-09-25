use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{PlayerSeason, PlayerSeasons, elo_range};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER, POINT_RADIUS,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend, padded_bounds,
};
use plotters::prelude::*;
use std::error::Error;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "elo_vs_games_season_";
const X_AXIS_TITLE: &str = "Games played in season";
const Y_AXIS_TITLE: &str = "Latest Elo in season";

pub(super) struct EloVersusGames;

impl Analysis for EloVersusGames {
    fn name(&self) -> &'static str {
        "elo-versus-games"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load_summaries(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;
        let seasons = data.seasons()?;

        let mut report = AnalysisReport::default();
        for season in seasons {
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &season.players)?;
            report.line(format!(
                "Season {}: {} rated players",
                season.season,
                season.players.len()
            ));
            report.line(format!("  Elo vs games: {}", path.display()));
        }
        Ok(report)
    }
}

fn draw_chart(path: &Path, season: u32, players: &[&PlayerSeason]) -> Result<(), Box<dyn Error>> {
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
    let (min_elo, max_elo) = elo_range(points.iter().map(|(_, elo)| *elo))?;
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
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
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
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
