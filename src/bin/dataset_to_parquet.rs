use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use polars::prelude::{
    FileWriteFormat, LazyFileListReader, LazyJsonLineReader, ParquetReader, ParquetWriteOptions,
    PlRefPath, SerReader, SinkDestination, SinkTarget, UnifiedSinkArgs,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let input_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dataset"));
    let output_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| input_dir.join("parquet"));
    if args.next().is_some() {
        return Err("usage: dataset_to_parquet [INPUT_DIR] [OUTPUT_DIR]".into());
    }

    let mut inputs = fs::read_dir(&input_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect::<Vec<_>>();
    inputs.sort();
    if inputs.is_empty() {
        return Err(format!("no .jsonl files found in {}", input_dir.display()).into());
    }

    fs::create_dir_all(&output_dir)?;

    let mut total_rows = 0_u64;
    for input_path in &inputs {
        let output_path = parquet_path(&output_dir, input_path)?;
        let rows = convert_and_count(input_path, &output_path)?;
        total_rows += rows;
        println!(
            "{}: {rows} rows -> {}",
            input_path.file_name().unwrap_or_default().to_string_lossy(),
            output_path.display()
        );
    }

    println!("Total: {total_rows} rows across {} seasons", inputs.len());
    Ok(())
}

fn parquet_path(output_dir: &Path, input_path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let stem = input_path
        .file_stem()
        .ok_or_else(|| format!("input has no file stem: {}", input_path.display()))?;
    let mut name = OsString::from(stem);
    name.push(".parquet");
    Ok(output_dir.join(name))
}

fn convert_and_count(input_path: &Path, output_path: &Path) -> Result<u64, Box<dyn Error>> {
    let stem = input_path
        .file_stem()
        .ok_or_else(|| format!("input has no file stem: {}", input_path.display()))?;
    let mut temporary_name = OsString::from(stem);
    temporary_name.push(format!(".{}.partial.parquet", std::process::id()));
    let temporary_path = output_path.with_file_name(temporary_name);
    if temporary_path.exists() {
        fs::remove_file(&temporary_path)?;
    }

    let conversion = (|| -> Result<(), Box<dyn Error>> {
        let source = PlRefPath::new(input_path.to_string_lossy().as_ref());
        let sink = PlRefPath::new(temporary_path.to_string_lossy().as_ref());
        let frame = LazyJsonLineReader::new(source)
            // Full-file inference retains fields and types that occur only in
            // sparse timeline payloads or exceptional historical records.
            .with_infer_schema_length(None)
            .with_batch_size(NonZeroUsize::new(4096))
            .low_memory(true)
            .finish()?;
        frame
            .sink(
                SinkDestination::File {
                    target: SinkTarget::Path(sink),
                },
                FileWriteFormat::Parquet(std::sync::Arc::new(ParquetWriteOptions::default())),
                UnifiedSinkArgs::default(),
            )?
            .collect()?;
        Ok(())
    })();

    if let Err(error) = conversion {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    fs::rename(&temporary_path, output_path)?;
    let mut reader = ParquetReader::new(File::open(output_path)?);
    Ok(reader.num_rows()? as u64)
}
