use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{PlayerSeason, PlayerSeasons, elo_range};
use crate::plot::{
    AXIS_DESC_FONT_SIZE, BASE_150, BASE_300, BASE_800, BLUE_600, CHART_HEIGHT, CHART_MARGIN,
    CHART_TITLE_SIZE, CHART_WIDTH, FONT_FAMILY, LABEL_AREA_SIZE, LEGEND_WIDTH, PAPER, POINT_RADIUS,
    TICK_LABEL_FONT_SIZE, Y_LABEL_AREA_SIZE, create_parent, draw_series_legend, format_correlation,
    padded_bounds, pearson_r,
};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "name_length_vs_elo_season_";
const X_AXIS_TITLE: &str = "Name length (characters)";
const Y_AXIS_TITLE: &str = "Final Elo in season";

pub(super) struct NameLengthVersusElo;

impl Analysis for NameLengthVersusElo {
    fn name(&self) -> &'static str {
        "name-length-versus-elo"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let data = PlayerSeasons::load_summaries(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
        )?;
        let seasons = data.seasons()?;

        let mut report = AnalysisReport::default();
        for season in seasons {
            let points = scatter_points(&season.players)?;
            let correlation = pearson_r(&points);
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{}.png", season.season));
            draw_chart(&path, season.season, &points, correlation)?;
            report.line(format!(
                "Season {}: {} rated players with a recorded name",
                season.season,
                points.len()
            ));
            report.line(format!(
                "  R (name length vs final Elo): {}",
                format_correlation(correlation)
            ));
            report.line(format!("  Name length vs Elo: {}", path.display()));
        }
        Ok(report)
    }
}

/// One point per rated player, pairing the character count of the name used at
/// their final rating with that final rating.
fn scatter_points(players: &[&PlayerSeason]) -> Result<Vec<(i64, f64)>, Box<dyn Error>> {
    let points: Vec<_> = players
        .iter()
        .filter_map(|player| {
            Some((
                i64::try_from(player.latest_name_length?).ok()?,
                player.latest_elo? as f64,
            ))
        })
        .collect();
    if points.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no players with both a name and a final Elo to plot",
        )
        .into());
    }
    Ok(points)
}

fn draw_chart(
    path: &Path,
    season: u32,
    points: &[(i64, f64)],
    correlation: Option<f64>,
) -> Result<(), Box<dyn Error>> {
    create_parent(path)?;
    let max_length = points
        .iter()
        .map(|(name_length, _)| *name_length)
        .max()
        .unwrap_or(1)
        .max(1);
    let x_max = max_length.saturating_add(max_length / 10).saturating_add(1);
    let (min_elo, max_elo) = elo_range(points.iter().map(|(_, elo)| *elo as i64))?;
    let (min_elo, max_elo) = padded_bounds(min_elo, max_elo);
    let caption_r = match correlation {
        Some(value) => format!("{value:.3}"),
        None => "n/a".to_owned(),
    };

    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!("Season {season}: Name Length vs Final Elo (r = {caption_r})"),
            (FONT_FAMILY, CHART_TITLE_SIZE, &BASE_800),
        )
        .margin(CHART_MARGIN)
        .x_label_area_size(LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0i64..x_max, min_elo as f64..max_elo as f64)?;

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
        .draw_series(points.iter().map(|(name_length, elo)| {
            Circle::new(
                (*name_length, *elo),
                POINT_RADIUS,
                BLUE_600.mix(0.25).filled(),
            )
        }))?
        .label("Players")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
