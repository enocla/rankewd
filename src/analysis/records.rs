use super::{Analysis, AnalysisInput, AnalysisReport};
use crate::dataset;
use std::error::Error;

pub(super) struct RecordCount;

impl Analysis for RecordCount {
    fn name(&self) -> &'static str {
        "records"
    }

    fn run(&self, input: &AnalysisInput<'_>) -> Result<AnalysisReport, Box<dyn Error>> {
        let mut report = AnalysisReport::default();
        for file in input.files() {
            let record_count = dataset::record_count(&file.path, input.seasons())?;
            report.line(format!(
                "{}: {record_count} records after default filters ({})",
                file.label,
                file.path.display()
            ));
        }
        Ok(report)
    }
}
