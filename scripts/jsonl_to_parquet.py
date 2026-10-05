"""Import raw JSONL into curated Parquet; invoke convert() from a Python session.

Runtime dependency: duckdb (use `direnv exec . uv run --with duckdb==1.5.6 python`).
There is no separate argument parser; the application's CLI remains in Rust.
"""
from pathlib import Path
from uuid import uuid4
import json
import duckdb


def convert(source: Path, destination: Path | None = None) -> dict:
    source = Path(source)
    destination = Path(destination) if destination else Path("output/curated") / source.with_suffix(".parquet").name
    if destination.exists():
        raise FileExistsError(f"Refusing to overwrite {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.{uuid4().hex}.partial")
    connection = duckdb.connect()
    connection.execute("SET threads=4")
    connection.execute("SET memory_limit='2GB'")
    # Infer from the complete export, not an initial sample full of nulls.
    # Older seasons store fractional pre-match Elo: retain it as DOUBLE.
    connection.execute("""CREATE TEMP TABLE ranked AS
        SELECT * REPLACE (
            list_transform(changes, x -> struct_pack(
                uuid := x.uuid, change := CAST(x.change AS BIGINT),
                eloRate := CAST(x.eloRate AS DOUBLE))) AS changes)
        FROM read_json_auto(?, format='newline_delimited', sample_size=-1)
        WHERE type = 2""", [str(source)])
    audit_sql = """SELECT count(*), count(DISTINCT id), min(season), max(season),
        count(*) FILTER (WHERE type != 2),
        bit_xor(hash(id, date, season, type, forfeited, decayed, result,
                     changes, players, timelines, completions, seed))
        FROM {table}"""
    expected = connection.execute(audit_sql.format(table="ranked")).fetchone()
    try:
        connection.table("ranked").write_parquet(str(temporary), compression="zstd", row_group_size=100_000)
        actual = connection.execute(audit_sql.format(table="read_parquet(?)"), [str(temporary)]).fetchone()
        if expected != actual:
            raise ValueError(f"Parquet verification failed: {expected} != {actual}")
        temporary.rename(destination)
    finally:
        if temporary.exists():
            temporary.unlink()  # Only this invocation's incomplete file.
        connection.close()
    result = dict(source=str(source), destination=str(destination), ranked_rows=expected[0],
                  unique_ids=expected[1], min_season=expected[2], max_season=expected[3],
                  non_ranked_rows=expected[4], research_field_checksum=expected[5])
    print(json.dumps(result), flush=True)
    return result
