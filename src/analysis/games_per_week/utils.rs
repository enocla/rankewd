use super::style::*;
use plotters::prelude::*;
use std::error::Error;
use std::io;
use std::path::Path;

pub fn iso_week_monday(year: i32, week: u32) -> Result<chrono::NaiveDate, Box<dyn Error>> {
    chrono::NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid ISO week").into())
}

pub fn draw_chart(path: &Path, data: &[(String, u64)]) -> Result<(), Box<dyn Error>> {
    let root = BitMapBackend::new(path, (CHART_WIDTH, CHART_HEIGHT)).into_drawing_area();
    root.fill(&PAPER)?;

    let max_count = data
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(0)
        .max(1);
    let last_index = i32::try_from(data.len().saturating_sub(1))?;
    let mut chart = ChartBuilder::on(&root)
        .caption(CHART_TITLE, (FONT_FAMILY, CHART_TITLE_SIZE))
        .margin(CHART_MARGIN)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(Y_LABEL_AREA_SIZE)
        .build_cartesian_2d(0..(last_index + 1), 0u64..(max_count + max_count / 10 + 1))?;

    let label_indices = data.len().min(MAX_X_LABELS).max(1);
    chart
        .configure_mesh()
        .x_desc(X_AXIS_TITLE)
        .y_desc(Y_AXIS_TITLE)
        .axis_style(BASE_800)
        .axis_desc_style((FONT_FAMILY, AXIS_DESC_FONT_SIZE, &BASE_800))
        .label_style((FONT_FAMILY, TICK_LABEL_FONT_SIZE, &BASE_800))
        .x_labels(label_indices)
        .x_label_formatter(&|index| {
            usize::try_from(*index)
                .ok()
                .and_then(|index| data.get(index))
                .map(|(label, _)| label.clone())
                .unwrap_or_default()
        })
        .bold_line_style(BASE_300)
        .light_line_style(BASE_150)
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            data.iter()
                .enumerate()
                .map(|(index, (_, count))| (index as i32, *count)),
            &BLUE_600,
        ))?
        .label("Games")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + LEGEND_WIDTH, y)], BLUE_600));
    chart.draw_series(data.iter().enumerate().map(|(index, (_, count))| {
        Circle::new((index as i32, *count), DATA_POINT_RADIUS, BLUE_600.filled())
    }))?;
    chart
        .configure_series_labels()
        .position(LEGEND_POSITION)
        .label_font((FONT_FAMILY, LEGEND_FONT_SIZE))
        .background_style(PAPER)
        .border_style(BASE_300)
        .draw()?;
    root.present()?;
    Ok(())
}
