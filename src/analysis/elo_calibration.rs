use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels};
use crate::plot::{
    BASE_800, BLUE_600, POINT_RADIUS, RED_600, SERIES_STROKE, canvas, chart_builder,
    draw_series_legend, legend_line, season_span, style_mesh,
};
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/elo_calibration.png";
/// Width of one rating-gap bin, in Elo points.
const GAP_BIN: i32 = 25;
/// Largest rating gap charted; wider gaps are too rare to bin.
const MAX_GAP: i32 = 500;
/// Matches a bin needs before its win rate is drawn.
const MIN_BIN_MATCHES: u64 = 200;

pub(super) struct EloCalibration;

#[derive(Clone, Copy, Default)]
struct GapBin {
    matches: u64,
    favourite_wins: u64,
    expected: f64,
}

impl Analysis for EloCalibration {
    fn name(&self) -> &'static str {
        "elo-calibration"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        let bin_count = usize::try_from(MAX_GAP / GAP_BIN)?;
        let mut bins = vec![GapBin::default(); bin_count];
        let mut total = GapBin::default();
        for duel in &duels.duels {
            let (Some((first, second)), Some(winner)) = (duel.elos(), duel.winner) else {
                continue;
            };
            let favourite = usize::from(second > first);
            let gap = (first - second).abs();
            if gap >= MAX_GAP {
                continue;
            }
            let expected = duel.expected_score(favourite).unwrap_or(0.5);
            let bin = &mut bins[usize::try_from(gap / GAP_BIN)?];
            for target in [bin, &mut total] {
                target.matches += 1;
                target.expected += expected;
                target.favourite_wins += u64::from(winner == favourite);
            }
        }
        if total.matches == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no rated matches").into());
        }

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        // The widest bin with enough matches carries the chart's headline.
        let headline = bins
            .iter()
            .enumerate()
            .filter(|(_, bin)| bin.matches >= MIN_BIN_MATCHES)
            .map(|(index, bin)| {
                (
                    index as i32 * GAP_BIN,
                    bin.expected / bin.matches as f64,
                    bin.favourite_wins as f64 / bin.matches as f64,
                )
            })
            .find(|(gap, _, _)| *gap >= 300)
            .unwrap_or((0, 0.5, 0.5));
        draw_chart(&output, &season_span(&seasons), &bins, headline)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: {} rated, decided matches with a gap below {MAX_GAP}",
            season_span(&seasons),
            total.matches
        ));
        report.line(format!(
            "  Higher-rated player won {:.2}% against an Elo expectation of {:.2}%",
            100.0 * total.favourite_wins as f64 / total.matches as f64,
            100.0 * total.expected / total.matches as f64
        ));
        for (index, bin) in bins.iter().enumerate().step_by(4) {
            if bin.matches >= MIN_BIN_MATCHES {
                report.line(format!(
                    "  Gap {:>3}-{:<3}: actual {:.1}% vs expected {:.1}% over {} matches",
                    index as i32 * GAP_BIN,
                    (index as i32 + 1) * GAP_BIN,
                    100.0 * bin.favourite_wins as f64 / bin.matches as f64,
                    100.0 * bin.expected / bin.matches as f64,
                    bin.matches
                ));
            }
        }
        report.line(format!("  Elo calibration: {}", output.display()));
        Ok(report)
    }
}

fn draw_chart(
    path: &Path,
    seasons: &str,
    bins: &[GapBin],
    (gap, expected, actual): (i32, f64, f64),
) -> Result<(), Box<dyn Error>> {
    let root = canvas(path)?;
    let title = format!(
        "{seasons}: {gap}+ Elo Favourites Win {:.0}%, Not {:.0}%",
        actual * 100.0,
        expected * 100.0
    );
    let mut chart = chart_builder(&root, &title)
        .build_cartesian_2d(0f64..f64::from(MAX_GAP), 0.4f64..1.12f64)?;
    style_mesh(
        &mut chart.configure_mesh(),
        "Pre-match Elo gap",
        "Higher-rated win rate",
    )
    .y_label_formatter(&|rate| {
        if *rate > 1.0 + 1e-9 {
            String::new()
        } else {
            format!("{:.0}%", rate * 100.0)
        }
    })
    .x_label_formatter(&|gap| format!("{gap:.0}"))
    .draw()?;

    let points = |value: fn(&GapBin) -> f64| -> Vec<(f64, f64)> {
        bins.iter()
            .enumerate()
            .filter(|(_, bin)| bin.matches >= MIN_BIN_MATCHES)
            .map(|(index, bin)| ((index as f64 + 0.5) * f64::from(GAP_BIN), value(bin)))
            .collect()
    };
    let expected = points(|bin| bin.expected / bin.matches as f64);
    let actual = points(|bin| bin.favourite_wins as f64 / bin.matches as f64);

    chart
        .draw_series(LineSeries::new(
            expected,
            BASE_800.stroke_width(SERIES_STROKE),
        ))?
        .label("Elo formula expectation")
        .legend(legend_line(BASE_800));
    chart
        .draw_series(LineSeries::new(
            actual.clone(),
            BLUE_600.stroke_width(SERIES_STROKE),
        ))?
        .label("Observed")
        .legend(legend_line(BLUE_600));
    chart.draw_series(
        actual
            .iter()
            .map(|point| Circle::new(*point, POINT_RADIUS + 2, RED_600.filled())),
    )?;
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
