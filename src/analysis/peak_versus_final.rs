use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_800, BLUE_600, RED_600, canvas, chart_builder, draw_series_legend, legend_box,
    legend_line, style_mesh, value_range,
};
use plotters::prelude::*;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

const OUTPUT_FILE_PREFIX: &str = "peak_vs_final_season_";
/// Rated matches a player needs before their peak means anything.
const MIN_RATED_MATCHES: u32 = 30;
/// Drop from peak that counts as a clear fall.
const FALL: i32 = 100;
const POINT_OPACITY: f64 = 0.35;

pub(super) struct PeakVersusFinal;

impl Analysis for PeakVersusFinal {
    fn name(&self) -> &'static str {
        "peak-versus-final"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let mut by_season: BTreeMap<u32, Vec<(f64, f64)>> = BTreeMap::new();
        for ((season, _), rating) in duels.ratings() {
            if rating.rated_matches >= MIN_RATED_MATCHES {
                by_season.entry(season).or_default().push((
                    f64::from(rating.peak_elo),
                    f64::from(rating.peak_elo - rating.final_elo),
                ));
            }
        }

        let mut report = AnalysisReport::default();
        for (season, points) in &by_season {
            let fallen = points.iter().filter(|p| p.1 >= f64::from(FALL)).count();
            let at_peak = points.iter().filter(|p| p.1 == 0.0).count();
            let share = fallen as f64 / points.len() as f64;
            let path = input
                .output()
                .sibling(&format!("{OUTPUT_FILE_PREFIX}{season}.png"));
            draw_chart(&path, *season, points, share)?;
            report.line(format!(
                "Season {season}: {} players with {MIN_RATED_MATCHES}+ rated matches; {:.1}% ended {FALL}+ below their peak, {:.1}% ended on it",
                points.len(),
                share * 100.0,
                100.0 * at_peak as f64 / points.len() as f64
            ));
            report.line(format!("  Peak vs final: {}", path.display()));
        }
        Ok(report)
    }
}

fn draw_chart(
    path: &Path,
    season: u32,
    points: &[(f64, f64)],
    fallen_share: f64,
) -> Result<(), Box<dyn Error>> {
    let x_range = value_range(points.iter().map(|p| p.0), 0.03, 0.03);
    let y_range = value_range(points.iter().map(|p| p.1), 0.03, 0.2);
    let root = canvas(path)?;
    let mut chart = chart_builder(
        &root,
        &format!(
            "Season {season}: {:.0}% End {FALL}+ Below Their Peak",
            fallen_share * 100.0
        ),
    )
    .build_cartesian_2d(x_range.clone(), y_range)?;
    style_mesh(
        &mut chart.configure_mesh(),
        "Peak Elo in season",
        "Peak minus final Elo",
    )
    .x_labels(8)
    .y_labels(8)
    .x_label_formatter(&|value| format!("{value:.0}"))
    .y_label_formatter(&|value| format!("{value:.0}"))
    .draw()?;

    let fall = f64::from(FALL);
    chart
        .draw_series(
            points
                .iter()
                .filter(|p| p.1 < fall)
                .map(|p| Circle::new(*p, 2, BLUE_600.mix(POINT_OPACITY).filled())),
        )?
        .label("Held within 100 of peak")
        .legend(legend_box(BLUE_600));
    chart
        .draw_series(
            points
                .iter()
                .filter(|p| p.1 >= fall)
                .map(|p| Circle::new(*p, 2, RED_600.mix(POINT_OPACITY).filled())),
        )?
        .label("Fell 100+ from peak")
        .legend(legend_box(RED_600));
    chart
        .draw_series(DashedLineSeries::new(
            [(x_range.start, fall), (x_range.end, fall)],
            12,
            8,
            BASE_800.stroke_width(2),
        ))?
        .label("100 below peak")
        .legend(legend_line(BASE_800));
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
