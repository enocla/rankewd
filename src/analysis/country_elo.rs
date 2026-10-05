use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset::{DuelColumns, load_duels, percentile};
use crate::plot::{
    BASE_800, BLUE_600, FONT_FAMILY, LEGEND_FONT_SIZE, RED_600, canvas, chart_builder,
    draw_series_legend, legend_line, season_span, style_mesh, value_range,
};
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use std::collections::HashMap;
use std::error::Error;
use std::io;
use std::path::Path;

const DEFAULT_OUTPUT_FILE: &str = "output/country_elo.png";
/// Rated matches a player-season needs before its final Elo counts.
const MIN_RATED_MATCHES: u32 = 10;
/// Countries charted, the ones with the most player-seasons.
const COUNTRIES: usize = 16;

pub(super) struct CountryElo;

struct CountryRow {
    code: String,
    players: usize,
    lower_quartile: f64,
    median: f64,
    upper_quartile: f64,
}

impl Analysis for CountryElo {
    fn name(&self) -> &'static str {
        "country-elo"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let duels = load_duels(
            input.files().iter().map(|file| file.path.as_path()),
            input.seasons(),
            DuelColumns::default(),
        )?;
        // Latest country each player-season lists.
        let mut countries: HashMap<(u32, u128), [u8; 2]> = HashMap::new();
        for duel in &duels.duels {
            for side in &duel.sides {
                if let Some(country) = side.country {
                    countries.insert((duel.season, side.uuid), country);
                }
            }
        }
        let mut by_country: HashMap<[u8; 2], Vec<f64>> = HashMap::new();
        let mut rated = 0usize;
        let mut without_country = 0usize;
        for (key, rating) in duels.ratings() {
            if rating.rated_matches < MIN_RATED_MATCHES {
                continue;
            }
            rated += 1;
            match countries.get(&key) {
                Some(country) => by_country
                    .entry(*country)
                    .or_default()
                    .push(f64::from(rating.final_elo)),
                None => without_country += 1,
            }
        }
        let mut largest: Vec<([u8; 2], Vec<f64>)> = by_country.into_iter().collect();
        largest.sort_unstable_by_key(|(code, elos)| (std::cmp::Reverse(elos.len()), *code));
        largest.truncate(COUNTRIES);
        if largest.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no rated player lists a country",
            )
            .into());
        }
        let mut rows: Vec<CountryRow> = largest
            .into_iter()
            .map(|(code, mut elos)| {
                elos.sort_unstable_by(f64::total_cmp);
                CountryRow {
                    code: String::from_utf8_lossy(&code).to_uppercase(),
                    players: elos.len(),
                    lower_quartile: percentile(&elos, 0.25),
                    median: percentile(&elos, 0.5),
                    upper_quartile: percentile(&elos, 0.75),
                }
            })
            .collect();
        rows.sort_unstable_by(|a, b| b.median.total_cmp(&a.median));

        let seasons = duels.seasons();
        let output = input.output().resolve(DEFAULT_OUTPUT_FILE);
        draw_chart(&output, &season_span(&seasons), &rows)?;

        let mut report = AnalysisReport::default();
        report.line(format!(
            "{}: {rated} player-seasons with {MIN_RATED_MATCHES}+ rated matches, {without_country} list no country",
            season_span(&seasons)
        ));
        for row in &rows {
            report.line(format!(
                "  {}: median {:.0} (IQR {:.0}-{:.0}), {} player-seasons",
                row.code, row.median, row.lower_quartile, row.upper_quartile, row.players
            ));
        }
        report.line(format!("  Country Elo: {}", output.display()));
        Ok(report)
    }
}

fn draw_chart(path: &Path, span: &str, rows: &[CountryRow]) -> Result<(), Box<dyn Error>> {
    let x_range = value_range(
        rows.iter()
            .flat_map(|row| [row.lower_quartile, row.upper_quartile]),
        0.05,
        0.45,
    );
    let count = rows.len() as f64;
    let labels: Vec<String> = rows.iter().map(|row| row.code.clone()).collect();
    let root = canvas(path)?;
    let mut chart = chart_builder(
        &root,
        &format!("{span}: Final Elo in the {} Largest Countries", rows.len()),
    )
    .build_cartesian_2d(x_range.clone(), count - 0.5..-0.5f64)?;
    style_mesh(&mut chart.configure_mesh(), "Final Elo", "Country")
        .disable_y_mesh()
        .x_labels(8)
        .y_labels(rows.len() * 2 + 1)
        .x_label_formatter(&|value| format!("{value:.0}"))
        .y_label_formatter(&|value| {
            let index = value.round();
            if (value - index).abs() < 1e-6 && index >= 0.0 {
                labels.get(index as usize).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        })
        .draw()?;

    chart
        .draw_series(rows.iter().enumerate().map(|(index, row)| {
            PathElement::new(
                vec![
                    (row.lower_quartile, index as f64),
                    (row.upper_quartile, index as f64),
                ],
                BLUE_600.stroke_width(6),
            )
        }))?
        .label("Interquartile range")
        .legend(legend_line(BLUE_600));
    chart
        .draw_series(
            rows.iter()
                .enumerate()
                .map(|(index, row)| Circle::new((row.median, index as f64), 8, RED_600.filled())),
        )?
        .label("Median")
        .legend(|(x, y)| Circle::new((x + 10, y), 6, RED_600.filled()));
    let left = Pos::new(HPos::Left, VPos::Center);
    chart.draw_series(rows.iter().enumerate().map(|(index, row)| {
        Text::new(
            format!("{} players", row.players),
            (
                row.upper_quartile + (x_range.end - x_range.start) * 0.01,
                index as f64,
            ),
            (FONT_FAMILY, LEGEND_FONT_SIZE)
                .into_font()
                .color(&BASE_800)
                .pos(left),
        )
    }))?;
    draw_series_legend(&mut chart)?;
    root.present()?;
    Ok(())
}
