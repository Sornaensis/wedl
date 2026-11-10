//! Disposable all-time author text cache and explicit request-local dated
//! selection. Author source text never supplies character knowledge or grants.
use crate::{Error, Result, model::World, validation::validate_world};
use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

mod author;
pub(crate) mod generation;
pub(crate) mod structured;
pub use author::{AuthorSearchSelection, search_with_hypotheses, search_with_selection};

const INDEX_SCHEMA: &str = "wedl-rust-fts/v1";

#[derive(Debug, Serialize)]
pub struct CompileReport {
    pub protocol: &'static str,
    pub schema: &'static str,
    pub fingerprint: String,
    pub revision: Option<String>,
    pub path: PathBuf,
    pub record_count: usize,
    pub indexed_count: usize,
    pub validation_complete: bool,
    pub scope: &'static str,
}

fn cache_path(world: &World) -> Result<PathBuf> {
    let directory = world.root.join(".wedl-rust");
    let path = directory.join("index.sqlite3");
    for candidate in [&directory, &path] {
        match fs::symlink_metadata(candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::InvalidInput(format!(
                    "cache paths must not be symlinks: {}",
                    candidate.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

fn validation_gate(world: &World) -> Result<()> {
    let report = validate_world(world);
    if !report.valid {
        return Err(Error::Validation(format!(
            "source failed core validation; run validate for diagnostics: {}",
            serde_json::to_string(&report)?
        )));
    }
    Ok(())
}

/// Validate, fully construct and check a new database, then atomically replace
/// the previous projection. Never open the previous index for writing.
pub fn compile(world: &World) -> Result<CompileReport> {
    validation_gate(world)?;
    let path = cache_path(world)?;
    let directory = path.parent().expect("cache file has a parent");
    fs::create_dir_all(directory)?;
    let temporary = tempfile::Builder::new()
        .prefix("index-")
        .suffix(".sqlite3")
        .tempfile_in(directory)?;
    let mut connection = Connection::open(temporary.path())?;
    connection.execute_batch(
        "PRAGMA journal_mode=DELETE;
         PRAGMA foreign_keys=ON;
         CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE records (
           rowid INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE,
           kind TEXT NOT NULL, title TEXT NOT NULL, source_path TEXT NOT NULL,
           frontmatter_yaml TEXT NOT NULL, body TEXT NOT NULL,
           blob_oid TEXT, indexed INTEGER NOT NULL CHECK(indexed IN (0,1))
         );
         CREATE VIRTUAL TABLE record_fts USING fts5(title, body, tokenize='unicode61');",
    )?;
    let transaction = connection.transaction()?;
    for (key, value) in [
        ("schema", INDEX_SCHEMA),
        ("fingerprint", world.fingerprint.as_str()),
        ("revision", world.revision.as_deref().unwrap_or("")),
        ("source_schema", crate::source::SOURCE_SCHEMA),
        ("scope", "author-all-time"),
    ] {
        transaction.execute("INSERT INTO metadata VALUES (?1,?2)", params![key, value])?;
    }
    let mut indexed_count = 0;
    {
        let mut record_statement = transaction.prepare(
            "INSERT INTO records(id,kind,title,source_path,frontmatter_yaml,body,blob_oid,indexed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?;
        let mut fts_statement =
            transaction.prepare("INSERT INTO record_fts(rowid,title,body) VALUES (?1,?2,?3)")?;
        for record in world.records.values() {
            // Hypotheses are noncanonical possibilities. They are inspectable
            // as source records but cannot affect the canonical text ranking.
            let indexed = record.kind != "hypothesis";
            record_statement.execute(params![
                record.id,
                record.kind,
                record.title,
                record.source_path,
                serde_yaml::to_string(&record.frontmatter)?,
                record.body,
                record.blob_oid,
                indexed,
            ])?;
            if indexed {
                fts_statement.execute(params![
                    transaction.last_insert_rowid(),
                    record.title,
                    record.body
                ])?;
                indexed_count += 1;
            }
        }
    }
    transaction.commit()?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(Error::Data(format!(
            "new index failed integrity check: {integrity}"
        )));
    }
    connection.execute(
        "INSERT INTO record_fts(record_fts) VALUES('integrity-check')",
        [],
    )?;
    connection
        .close()
        .map_err(|(_, error)| Error::Sqlite(error))?;
    temporary.as_file().sync_all()?;
    // tempfile uses a same-directory atomic rename with replacement.
    temporary
        .persist(&path)
        .map_err(|error| Error::Io(error.error))?;
    #[cfg(feature = "snapshot-baseline")]
    crate::snapshot_baseline::record(|stats| {
        stats.fts_builds += 1;
        stats.projected_records += world.records.len();
        stats.projected_fts_rows += indexed_count;
    });
    Ok(CompileReport {
        protocol: crate::CONTRACT,
        schema: INDEX_SCHEMA,
        fingerprint: world.fingerprint.clone(),
        revision: world.revision.clone(),
        path,
        record_count: world.records.len(),
        indexed_count,
        validation_complete: false,
        scope: "author-all-time",
    })
}

fn matching_index(world: &World) -> Result<Option<Connection>> {
    #[cfg(feature = "snapshot-baseline")]
    crate::snapshot_baseline::record(|stats| stats.fts_index_probes += 1);
    let path = cache_path(world)?;
    if !path.exists() {
        return Ok(None);
    }
    let connection = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    for (key, expected) in [
        ("schema", INDEX_SCHEMA),
        ("source_schema", crate::source::SOURCE_SCHEMA),
        ("fingerprint", world.fingerprint.as_str()),
        ("revision", world.revision.as_deref().unwrap_or("")),
    ] {
        let actual: std::result::Result<String, _> =
            connection.query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
                row.get(0)
            });
        if actual.as_deref().ok() != Some(expected) {
            return Ok(None);
        }
    }
    #[cfg(feature = "snapshot-baseline")]
    crate::snapshot_baseline::record(|stats| stats.fts_index_quick_checks += 1);
    let integrity: std::result::Result<String, _> =
        connection.query_row("PRAGMA quick_check", [], |row| row.get(0));
    if integrity.as_deref().ok() != Some("ok") {
        return Ok(None);
    }
    // An otherwise healthy file with missing projection tables is still stale.
    if connection
        .prepare("SELECT id FROM records LIMIT 0")
        .is_err()
        || connection
            .prepare("SELECT rowid FROM record_fts LIMIT 0")
            .is_err()
    {
        return Ok(None);
    }
    #[cfg(feature = "snapshot-baseline")]
    crate::snapshot_baseline::record(|stats| stats.fts_index_matches += 1);
    Ok(Some(connection))
}

/// Read cache metadata without creating directories, files or a database.
pub fn status(world: &World) -> Result<Value> {
    let path = cache_path(world)?;
    let ready = matching_index(world)?.is_some();
    Ok(json!({
        "path":path, "ready":ready, "schema":INDEX_SCHEMA,
        "sourceFingerprint":world.fingerprint, "revision":world.revision,
        "scope":"author-all-time", "validationComplete":false,
    }))
}

/// Literal-word AND search. User input is quoted into FTS tokens and SQL
/// parameters, so SQL/FTS operators are not accepted as executable syntax.
pub fn search(world: &World, query: &str, limit: usize, kind: Option<&str>) -> Result<Value> {
    if query.trim().is_empty() || query.len() > 4096 {
        return Err(Error::InvalidInput(
            "query must contain 1–4096 UTF-8 bytes".into(),
        ));
    }
    if !(1..=100).contains(&limit) {
        return Err(Error::InvalidInput(
            "limit must be between 1 and 100".into(),
        ));
    }
    let words: Vec<_> = query.split_whitespace().collect();
    if words.len() > 64 {
        return Err(Error::InvalidInput(
            "query must contain at most 64 words".into(),
        ));
    }
    let expression = words
        .iter()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    validation_gate(world)?;
    let connection = match matching_index(world)? {
        Some(connection) => connection,
        None => {
            compile(world)?;
            matching_index(world)?
                .ok_or_else(|| Error::Conflict("index changed during search; retry".into()))?
        }
    };
    let mut statement = connection.prepare(
        "SELECT r.id,r.kind,r.title,r.source_path,r.blob_oid,
                snippet(record_fts,1,'','',' … ',24),bm25(record_fts,4.0,1.0)
         FROM record_fts JOIN records r ON r.rowid=record_fts.rowid
         WHERE record_fts MATCH ?1 AND (?2 IS NULL OR r.kind=?2)
         ORDER BY bm25(record_fts,4.0,1.0),r.id LIMIT ?3",
    )?;
    let rows = statement.query_map(params![expression, kind, limit as i64], |row| {
        Ok(json!({
            "id":row.get::<_,String>(0)?, "kind":row.get::<_,String>(1)?,
            "title":row.get::<_,String>(2)?, "sourcePath":row.get::<_,String>(3)?,
            "blobOid":row.get::<_,Option<String>>(4)?, "snippet":row.get::<_,String>(5)?,
            "score":row.get::<_,f64>(6)?,
        }))
    })?;
    let items: Vec<Value> = rows.collect::<std::result::Result<_, _>>()?;
    Ok(json!({
        "protocol":crate::CONTRACT,"scope":"author-all-time","query":query,
        "fingerprint":world.fingerprint,"revision":world.revision,
        "items":items,"limit":limit,"hypothesesIncluded":false,
        "notice":"Text search has no as-of or character-privacy semantics.",
    }))
}

/// Search the current all-time author source corpus. The legacy lexical route
/// retains its disposable SQLite index; semantic and hybrid routes fit only
/// the selected title/body corpus in memory for this request.
pub fn search_with_mode(
    world: &World,
    query: &str,
    limit: usize,
    kind: Option<&str>,
    mode: crate::retrieval::Mode,
) -> Result<Value> {
    use crate::retrieval::{self, Document, Mode};
    if mode == Mode::Lexical {
        return search(world, query, limit, kind);
    }
    retrieval::validate_request(query, limit)?;
    validation_gate(world)?;
    let records: Vec<_> = world
        .records
        .values()
        .filter(|record| record.kind != "hypothesis" && kind.is_none_or(|kind| record.kind == kind))
        .collect();
    let mut bytes = query.len();
    // Admit source fields before making owned copies or constructing vector
    // text. The shared ranker subsequently charges the exact training union.
    for record in &records {
        bytes = bytes
            .checked_add(record.id.len())
            .and_then(|value| value.checked_add(record.title.len()))
            .and_then(|value| value.checked_add(record.body.len()))
            .and_then(|value| value.checked_add(1))
            .ok_or_else(retrieval::bounds_error)?;
    }
    if records.len() > retrieval::MAX_INPUTS || bytes > retrieval::MAX_BYTES {
        return Err(retrieval::bounds_error());
    }
    let mut training = std::collections::BTreeSet::new();
    for record in &records {
        // Stage one already byte-bounded text, then charge every distinct fit
        // input before constructing the owned candidate projection.
        let text = if record.body.is_empty() {
            record.title.clone()
        } else {
            format!("{}\n{}", record.title, record.body)
        };
        if !training.contains(&text) {
            bytes = bytes
                .checked_add(text.len())
                .ok_or_else(retrieval::bounds_error)?;
            if bytes > retrieval::MAX_BYTES
                || records.len() + training.len() + 1 > retrieval::MAX_INPUTS
            {
                return Err(retrieval::bounds_error());
            }
            training.insert(text);
        }
    }
    let training: Vec<_> = training.into_iter().collect();
    let documents: Vec<_> = records
        .iter()
        .map(|record| Document {
            key: record.id.clone(),
            title: record.title.clone(),
            text: record.body.clone(),
        })
        .collect();
    let ranked = retrieval::rank(query, &documents, &training, mode, limit, true)?;
    let items: Vec<_> = ranked
        .hits
        .iter()
        .map(|hit| {
            let record = records[hit.index];
            let mut item = json!({
                "id":record.id, "kind":record.kind, "title":record.title,
                "sourcePath":record.source_path, "blobOid":record.blob_oid,
            });
            hit.write_metadata(&mut item);
            item
        })
        .collect();
    Ok(json!({
        "protocol":crate::CONTRACT, "scope":"author-all-time", "query":query,
        "fingerprint":world.fingerprint, "revision":world.revision,
        "items":items, "limit":limit, "total":ranked.total,
        "hypothesesIncluded":false, "mode":mode.as_str(),
        "retrieval":retrieval::model_metadata(ranked.dimensions),
        "notice":"Source search has no as-of or character-privacy semantics.",
    }))
}
