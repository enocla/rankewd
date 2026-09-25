use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, List, Row};
use parquet::schema::parser::parse_message_type;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::File;
use std::io;
use std::path::Path;

pub(crate) type ParquetFile = SerializedFileReader<File>;

pub(crate) fn open(path: &Path) -> Result<ParquetFile, Box<dyn Error>> {
    Ok(SerializedFileReader::new(File::open(path)?)?)
}

pub(crate) fn record_count(path: &Path) -> Result<i64, Box<dyn Error>> {
    Ok(open(path)?.metadata().file_metadata().num_rows())
}

pub(crate) fn record_count_for_seasons(
    path: &Path,
    seasons: &[u32],
) -> Result<i64, Box<dyn Error>> {
    let requested = seasons.iter().copied().collect::<BTreeSet<_>>();
    if requested.is_empty() {
        return Ok(0);
    }

    let reader = open(path)?;
    let projection = parse_message_type("message root { optional int64 season; }")?;
    let mut count = 0i64;
    for row in reader.get_row_iter(Some(projection))? {
        let row = row?;
        let season = integer_field(&row, "season")?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing season column"))?;
        let season = u32::try_from(season).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "season is outside u32 range")
        })?;
        if requested.contains(&season) {
            count = count
                .checked_add(1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "row count overflow"))?;
        }
    }
    Ok(count)
}

pub(crate) fn integer_field(row: &Row, name: &str) -> Result<Option<i64>, Box<dyn Error>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Long(value)) => Ok(Some(*value)),
        Some(Field::Int(value)) => Ok(Some(i64::from(*value))),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => Err(missing_field(name).into()),
    }
}

pub(crate) fn string_field<'a>(
    row: &'a Row,
    name: &str,
) -> Result<Option<&'a str>, Box<dyn Error>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Str(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => Err(missing_field(name).into()),
    }
}

pub(crate) fn list_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a List>, Box<dyn Error>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::ListInternal(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => Err(missing_field(name).into()),
    }
}

pub(crate) fn group_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a Row>, Box<dyn Error>> {
    let value = find_field(row, name)?;
    match value {
        Some(Field::Group(value)) => Ok(Some(value)),
        Some(Field::Null) => Ok(None),
        Some(value) => Err(invalid_field(name, value).into()),
        None => Err(missing_field(name).into()),
    }
}

fn find_field<'a>(row: &'a Row, name: &str) -> Result<Option<&'a Field>, io::Error> {
    Ok(row
        .get_column_iter()
        .find_map(|(column, value)| (column == name).then_some(value)))
}

fn missing_field(name: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("missing {name} column"))
}

pub(crate) fn invalid_field(name: &str, value: &Field) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unexpected value for {name}: {value:?}"),
    )
}
