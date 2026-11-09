//! Bounded, request-local retrieval over material already admitted by a caller.
//!
//! This module deliberately accepts no world, source path, scope, or grants.
//! Training texts determine the LSA basis; candidate texts and the query are
//! projected into that basis and never alter its vocabulary or IDF values.
use crate::{Error, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

pub(crate) const MAX_INPUTS: usize = 1024;
pub(crate) const MAX_BYTES: usize = 524_288;
const MAX_TRAINING: usize = 512;
const MAX_FEATURES: usize = 8192;
const REQUESTED_DIMENSIONS: usize = 192;
const MAX_ENTRIES: usize = 131_072;
const MAX_KEYS: usize = 65_536;
const MAX_KEY_CHARS: usize = 1_048_576;
const MAX_PROJECTION_WORK: usize = 268_435_456;
const MAX_GRAM_WORK: usize = 268_435_456;
const MAX_JACOBI_SWEEPS: usize = 32;
const MAX_JACOBI_WORK: u64 = 4_294_967_296;
const RRF_K: f64 = 60.0;
const EPSILON: f64 = 1e-12;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Lexical,
    Semantic,
    Hybrid,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Mode {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "lexical" => Ok(Self::Lexical),
            "semantic" => Ok(Self::Semantic),
            "hybrid" => Ok(Self::Hybrid),
            _ => Err(Error::InvalidInput(
                "mode must be lexical, semantic, or hybrid".into(),
            )),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Document {
    pub key: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug)]
pub(crate) struct Hit {
    pub index: usize,
    pub snippet: String,
    pub score: f64,
    pub lexical_rank: Option<usize>,
    pub lexical_score: Option<f64>,
    pub semantic_rank: Option<usize>,
    pub semantic_score: Option<f64>,
}

impl Hit {
    pub(crate) fn write_metadata(&self, item: &mut Value) {
        item["snippet"] = json!(self.snippet);
        item["score"] = json!(self.score);
        item["lexicalRank"] = json!(self.lexical_rank);
        item["lexicalScore"] = json!(self.lexical_score);
        item["semanticRank"] = json!(self.semantic_rank);
        item["semanticScore"] = json!(self.semantic_score);
    }
}

pub(crate) struct Ranked {
    pub hits: Vec<Hit>,
    pub total: usize,
    pub dimensions: usize,
}

pub(crate) fn model_metadata(dimensions: usize) -> Value {
    json!({
        "provider":"lsa", "features":"word-unigram-bigram-tfidf",
        "projection":"truncated-svd", "solver":"gram-jacobi",
        "dimensions":dimensions, "requestedDimensions":REQUESTED_DIMENSIONS,
        "maxFeatures":MAX_FEATURES, "normalized":true, "ephemeral":true,
        "fusion":{"method":"reciprocal-rank", "k":60, "lexicalWeight":1, "semanticWeight":1}
    })
}

pub(crate) fn bounds_error() -> Error {
    Error::InvalidInput("semantic retrieval exceeds its complete input or work bounds".into())
}

fn fit_error() -> Error {
    Error::InvalidInput("semantic retrieval fit is unavailable".into())
}

pub(crate) fn validate_request(query: &str, limit: usize) -> Result<()> {
    if query.trim().is_empty() || query.len() > 4096 {
        return Err(Error::InvalidInput(
            "query must contain 1–4096 UTF-8 bytes".into(),
        ));
    }
    if query.split_whitespace().count() > 64 {
        return Err(Error::InvalidInput(
            "query must contain at most 64 words".into(),
        ));
    }
    if !(1..=100).contains(&limit) {
        return Err(Error::InvalidInput(
            "limit must be between 1 and 100".into(),
        ));
    }
    Ok(())
}

/// Both lanes rank the complete admitted candidate set before the result limit.
/// Inputs are charged before deduplication; targets do not train the model.
pub(crate) fn rank(
    query: &str,
    documents: &[Document],
    training: &[String],
    mode: Mode,
    limit: usize,
    author_title_weight: bool,
) -> Result<Ranked> {
    validate_request(query, limit)?;
    let mut ranked = rank_complete(query, documents, training, mode, author_title_weight)?;
    ranked.hits.truncate(limit);
    Ok(ranked)
}

/// Complete bounded candidate window for callers that apply grouping after
/// ranking. Group membership must not change either lane's statistics or ranks.
pub(crate) fn rank_complete(
    query: &str,
    documents: &[Document],
    training: &[String],
    mode: Mode,
    author_title_weight: bool,
) -> Result<Ranked> {
    validate_request(query, 1)?;
    let mut keys = BTreeSet::new();
    let mut bytes = query.len();
    if documents
        .len()
        .checked_add(training.len())
        .is_none_or(|n| n > MAX_INPUTS)
    {
        return Err(bounds_error());
    }
    for text in training {
        bytes = bytes.checked_add(text.len()).ok_or_else(bounds_error)?;
    }
    for document in documents {
        if document.key.is_empty() || !keys.insert(document.key.as_str()) {
            return Err(Error::Data(
                "retrieval candidates require unique nonempty keys".into(),
            ));
        }
        for length in [
            document.key.len(),
            document.title.len(),
            document.text.len(),
            1,
        ] {
            bytes = bytes.checked_add(length).ok_or_else(bounds_error)?;
        }
    }
    if bytes > MAX_BYTES {
        return Err(bounds_error());
    }

    let mut hits: BTreeMap<usize, Hit> = BTreeMap::new();
    // Fit first: an unavailable semantic lane cannot become a lexical fallback.
    let (semantic, dimensions) = if mode != Mode::Lexical {
        semantic_lane(query, documents, training)?
    } else {
        (Vec::new(), 0)
    };
    for (position, (index, score)) in semantic.into_iter().enumerate() {
        let lane_rank = position + 1;
        hits.insert(
            index,
            Hit {
                index,
                snippet: documents[index].text.chars().take(300).collect(),
                score: if mode == Mode::Hybrid {
                    1.0 / (RRF_K + lane_rank as f64)
                } else {
                    score
                },
                lexical_rank: None,
                lexical_score: None,
                semantic_rank: Some(lane_rank),
                semantic_score: Some(score),
            },
        );
    }
    if mode != Mode::Semantic {
        for (position, (index, snippet, score)) in
            lexical_lane(query, documents, author_title_weight)?
                .into_iter()
                .enumerate()
        {
            let lane_rank = position + 1;
            let hit = hits.entry(index).or_insert_with(|| Hit {
                index,
                snippet: String::new(),
                score: 0.0,
                lexical_rank: None,
                lexical_score: None,
                semantic_rank: None,
                semantic_score: None,
            });
            hit.snippet = snippet;
            hit.lexical_rank = Some(lane_rank);
            hit.lexical_score = Some(score);
            hit.score += if mode == Mode::Hybrid {
                1.0 / (RRF_K + lane_rank as f64)
            } else {
                score
            };
        }
    }
    let mut hits: Vec<_> = hits.into_values().collect();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| documents[left.index].key.cmp(&documents[right.index].key))
    });
    let total = hits.len();
    Ok(Ranked {
        hits,
        total,
        dimensions,
    })
}

fn lexical_lane(
    query: &str,
    documents: &[Document],
    author_title_weight: bool,
) -> Result<Vec<(usize, String, f64)>> {
    let mut connection = Connection::open_in_memory()?;
    connection.execute_batch("CREATE VIRTUAL TABLE admitted_fts USING fts5(title,text,key UNINDEXED,tokenize='unicode61');")?;
    let transaction = connection.transaction()?;
    {
        let mut insert = transaction
            .prepare("INSERT INTO admitted_fts(rowid,title,text,key) VALUES(?1,?2,?3,?4)")?;
        for (index, document) in documents.iter().enumerate() {
            insert.execute(params![
                index as i64 + 1,
                document.title,
                document.text,
                document.key
            ])?;
        }
    }
    transaction.commit()?;
    let expression = query
        .split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    let mut statement = connection.prepare(
        "SELECT rowid,snippet(admitted_fts,1,'','',' … ',24),bm25(admitted_fts,?2,1.0) \
         FROM admitted_fts WHERE admitted_fts MATCH ?1 \
         ORDER BY bm25(admitted_fts,?2,1.0),key",
    )?;
    let rows = statement.query_map(
        params![expression, if author_title_weight { 4.0 } else { 0.0 }],
        |row| {
            Ok((
                row.get::<_, i64>(0)? as usize - 1,
                row.get(1)?,
                -row.get::<_, f64>(2)?,
            ))
        },
    )?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

type Counts = BTreeMap<String, usize>;
type SparseRow = Vec<(usize, f64)>;

fn stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "but"
            | "by"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "he"
            | "her"
            | "hers"
            | "him"
            | "his"
            | "how"
            | "i"
            | "in"
            | "is"
            | "it"
            | "its"
            | "me"
            | "my"
            | "of"
            | "on"
            | "or"
            | "our"
            | "she"
            | "that"
            | "the"
            | "their"
            | "them"
            | "they"
            | "this"
            | "to"
            | "was"
            | "we"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "with"
            | "you"
            | "your"
    )
}

fn features(text: &str) -> Counts {
    let tokens: Vec<String> = text
        .split(|character: char| {
            !(character.is_alphanumeric() || matches!(character, '_' | '\'' | '-'))
        })
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .filter(|token| !stopword(token))
        .collect();
    let mut counts = Counts::new();
    for token in &tokens {
        *counts.entry(token.clone()).or_default() += 1;
    }
    for pair in tokens.windows(2) {
        *counts
            .entry(format!("{}::{}", pair[0], pair[1]))
            .or_default() += 1;
    }
    counts
}

fn normalize(values: &mut [f64]) -> Result<()> {
    let norm = values.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !norm.is_finite() {
        return Err(fit_error());
    }
    if norm <= EPSILON {
        values.fill(0.0);
    } else {
        values.iter_mut().for_each(|value| *value /= norm);
    }
    Ok(())
}

fn transform(counts: &Counts, vocabulary: &BTreeMap<&str, usize>, idf: &[f64]) -> SparseRow {
    let mut row: Vec<_> = counts
        .iter()
        .filter_map(|(term, count)| {
            vocabulary
                .get(term.as_str())
                .map(|index| (*index, (1.0 + (*count as f64).ln()) * idf[*index]))
        })
        .collect();
    row.sort_unstable_by_key(|(index, _)| *index);
    let norm = row
        .iter()
        .map(|(_, value)| value * value)
        .sum::<f64>()
        .sqrt();
    if norm > EPSILON {
        row.iter_mut().for_each(|(_, value)| *value /= norm);
    }
    row
}

fn sparse_dot(left: &SparseRow, right: &SparseRow) -> f64 {
    let (mut l, mut r, mut result) = (0, 0, 0.0);
    while l < left.len() && r < right.len() {
        match left[l].0.cmp(&right[r].0) {
            std::cmp::Ordering::Less => l += 1,
            std::cmp::Ordering::Greater => r += 1,
            std::cmp::Ordering::Equal => {
                result += left[l].1 * right[r].1;
                l += 1;
                r += 1;
            }
        }
    }
    result
}

fn checked_work(factors: &[usize], bound: usize) -> Result<()> {
    let mut result = 1usize;
    for factor in factors {
        result = result.checked_mul(*factor).ok_or_else(bounds_error)?;
        if result > bound {
            return Err(bounds_error());
        }
    }
    Ok(())
}

fn semantic_lane(
    query: &str,
    documents: &[Document],
    training: &[String],
) -> Result<(Vec<(usize, f64)>, usize)> {
    let training: BTreeSet<&str> = training.iter().map(String::as_str).collect();
    if training.len() > MAX_TRAINING {
        return Err(bounds_error());
    }
    if training.is_empty() {
        if documents.is_empty() {
            return Ok((Vec::new(), 0));
        }
        return Err(Error::InvalidInput(
            "semantic retrieval requires an admitted training corpus".into(),
        ));
    }
    let target_texts: Vec<String> = documents
        .iter()
        .map(|document| {
            if document.title.is_empty() {
                document.text.clone()
            } else if document.text.is_empty() {
                document.title.clone()
            } else {
                format!("{}\n{}", document.title, document.text)
            }
        })
        .collect();
    let mut texts: BTreeSet<&str> = training.clone();
    texts.extend(target_texts.iter().map(String::as_str));
    texts.insert(query);
    let mut all_counts = BTreeMap::new();
    let (mut entries, mut characters) = (0usize, 0usize);
    let mut keys = BTreeSet::new();
    for text in texts {
        let counts = features(text);
        entries = entries.checked_add(counts.len()).ok_or_else(bounds_error)?;
        characters = characters
            .checked_add(counts.keys().map(|key| key.chars().count()).sum::<usize>())
            .ok_or_else(bounds_error)?;
        if entries > MAX_ENTRIES || characters > MAX_KEY_CHARS {
            return Err(bounds_error());
        }
        keys.extend(counts.keys().cloned());
        if keys.len() > MAX_KEYS {
            return Err(bounds_error());
        }
        all_counts.insert(text, counts);
    }
    let mut frequencies: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for text in &training {
        for (term, count) in &all_counts[text] {
            let frequency = frequencies.entry(term).or_default();
            frequency.0 += 1;
            frequency.1 += count;
        }
    }
    let mut terms: Vec<_> = frequencies.keys().copied().collect();
    terms.sort_by(|left, right| {
        frequencies[right]
            .cmp(&frequencies[left])
            .then_with(|| left.cmp(right))
    });
    terms.truncate(MAX_FEATURES);
    if terms.is_empty() {
        terms.push("__empty__");
    }
    let n = training.len();
    let f = terms.len();
    let dimensions = REQUESTED_DIMENSIONS.min(n.saturating_sub(1).min(f.saturating_sub(1)).max(1));
    let vocabulary: BTreeMap<&str, usize> = terms
        .iter()
        .enumerate()
        .map(|(index, term)| (*term, index))
        .collect();
    // Components and query/target projection both use sparse input rows. Price
    // the two passes together, not an unrelated dense N*F*D estimate.
    let selected_entries = |counts: &Counts| {
        counts
            .keys()
            .filter(|term| vocabulary.contains_key(term.as_str()))
            .count()
    };
    let training_entries = training
        .iter()
        .map(|text| selected_entries(&all_counts[text]))
        .sum::<usize>();
    let target_entries = target_texts
        .iter()
        .map(|text| selected_entries(&all_counts[text.as_str()]))
        .sum::<usize>()
        .checked_add(selected_entries(&all_counts[query]))
        .ok_or_else(bounds_error)?;
    checked_work(
        &[
            training_entries
                .checked_add(target_entries)
                .ok_or_else(bounds_error)?,
            dimensions,
        ],
        MAX_PROJECTION_WORK,
    )?;
    checked_work(&[8, f, dimensions], 16_777_216)?;
    checked_work(&[8, documents.len() + 1, dimensions], 4_194_304)?;
    // A sparse Gram dot visits no more than the sum of both row lengths.
    checked_work(&[n, training_entries.max(1)], MAX_GRAM_WORK)?;
    let jacobi_work = (n as u64)
        .checked_pow(3)
        .and_then(|work| work.checked_mul(MAX_JACOBI_SWEEPS as u64))
        .ok_or_else(bounds_error)?;
    if jacobi_work > MAX_JACOBI_WORK {
        return Err(bounds_error());
    }
    if frequencies.is_empty() {
        // The placeholder vocabulary is a matrix-shape device, not a learned
        // token that a query or candidate may use to create a nonzero vector.
        return Ok((Vec::new(), dimensions));
    }
    let idf: Vec<_> = terms
        .iter()
        .map(|term| {
            ((1.0 + n as f64)
                / (1.0 + frequencies.get(term).map_or(0, |frequency| frequency.0) as f64))
                .ln()
                + 1.0
        })
        .collect();
    let rows: Vec<_> = training
        .iter()
        .map(|text| transform(&all_counts[text], &vocabulary, &idf))
        .collect();
    let components = components(&rows, f, dimensions)?;
    let project = |counts: &Counts| -> Result<Vec<f64>> {
        let row = transform(counts, &vocabulary, &idf);
        let mut vector: Vec<_> = components
            .iter()
            .map(|component| {
                row.iter()
                    .map(|(column, value)| value * component[*column])
                    .sum()
            })
            .collect();
        normalize(&mut vector)?;
        Ok(vector)
    };
    let query_vector = project(&all_counts[query])?;
    if query_vector.iter().all(|value| *value == 0.0) {
        return Ok((Vec::new(), components.len()));
    }
    let mut scores = Vec::with_capacity(documents.len());
    for (index, text) in target_texts.iter().enumerate() {
        let vector = project(&all_counts[text.as_str()])?;
        let score = query_vector
            .iter()
            .zip(vector)
            .map(|(left, right)| left * right)
            .sum::<f64>()
            .clamp(-1.0, 1.0);
        if !score.is_finite() {
            return Err(fit_error());
        }
        scores.push((index, score));
    }
    scores.sort_by(|(left_index, left), (right_index, right)| {
        right
            .total_cmp(left)
            .then_with(|| documents[*left_index].key.cmp(&documents[*right_index].key))
    });
    Ok((scores, components.len()))
}

/// Right singular vectors of normalized TF-IDF A, recovered from eigenvectors
/// of A Aᵀ. This is exact truncated LSA up to the bounded solver's tolerance.
fn components(rows: &[SparseRow], feature_count: usize, requested: usize) -> Result<Vec<Vec<f64>>> {
    let n = rows.len();
    if n == 1 || feature_count == 1 {
        let mut component = vec![0.0; feature_count];
        component[0] = 1.0;
        return Ok(vec![component]);
    }
    let mut gram = vec![0.0; n * n];
    for left in 0..n {
        for right in 0..=left {
            let value = sparse_dot(&rows[left], &rows[right]);
            gram[left * n + right] = value;
            gram[right * n + left] = value;
        }
    }
    let (eigenvalues, eigenvectors) = jacobi(gram, n, MAX_JACOBI_SWEEPS)?;
    let mut order: Vec<_> = (0..n).collect();
    order.sort_by(|left, right| {
        eigenvalues[*right]
            .total_cmp(&eigenvalues[*left])
            .then_with(|| left.cmp(right))
    });
    let maximum = eigenvalues[order[0]].max(0.0);
    if eigenvalues
        .iter()
        .any(|value| !value.is_finite() || *value < -1e-9 * maximum.max(1.0))
    {
        return Err(fit_error());
    }
    let mut result = Vec::with_capacity(requested);
    for index in order.into_iter().take(requested) {
        let eigenvalue = eigenvalues[index];
        let mut component = vec![0.0; feature_count];
        if eigenvalue > EPSILON * maximum.max(1.0) {
            let scale = eigenvalue.sqrt();
            for (row_index, row) in rows.iter().enumerate() {
                let factor = eigenvectors[row_index * n + index] / scale;
                for (column, value) in row {
                    component[*column] += factor * value;
                }
            }
            normalize(&mut component)?;
        }
        result.push(component);
    }
    Ok(result)
}

/// Cyclic Jacobi diagonalization for a small, symmetric, positive-semidefinite
/// Gram matrix. Fixed ordering and a hard sweep bound make the fit reproducible.
fn jacobi(mut matrix: Vec<f64>, n: usize, max_sweeps: usize) -> Result<(Vec<f64>, Vec<f64>)> {
    let mut vectors = vec![0.0; n * n];
    for index in 0..n {
        vectors[index * n + index] = 1.0;
    }
    for sweep in 0..=max_sweeps {
        let diagonal = (0..n)
            .map(|index| matrix[index * n + index].abs())
            .sum::<f64>();
        let off = (0..n)
            .flat_map(|row| ((row + 1)..n).map(move |column| (row, column)))
            .map(|(row, column)| matrix[row * n + column].abs())
            .sum::<f64>();
        if !diagonal.is_finite() || !off.is_finite() {
            return Err(fit_error());
        }
        if off <= 1e-12 * diagonal.max(1.0) {
            return Ok((
                (0..n).map(|index| matrix[index * n + index]).collect(),
                vectors,
            ));
        }
        if sweep == max_sweeps {
            return Err(fit_error());
        }
        let threshold = if sweep < 4 {
            0.2 * off / (n * n) as f64
        } else {
            0.0
        };
        for p in 0..n {
            for q in (p + 1)..n {
                let apq = matrix[p * n + q];
                if apq.abs() <= threshold {
                    continue;
                }
                let app = matrix[p * n + p];
                let aqq = matrix[q * n + q];
                if apq.abs() <= f64::EPSILON * (app.abs() + aqq.abs()).max(1.0) {
                    matrix[p * n + q] = 0.0;
                    matrix[q * n + p] = 0.0;
                    continue;
                }
                let tau = (aqq - app) / (2.0 * apq);
                let tangent = if tau >= 0.0 {
                    1.0 / (tau + tau.hypot(1.0))
                } else {
                    -1.0 / (-tau + tau.hypot(1.0))
                };
                let cosine = 1.0 / (1.0 + tangent * tangent).sqrt();
                let sine = tangent * cosine;
                matrix[p * n + p] = app - tangent * apq;
                matrix[q * n + q] = aqq + tangent * apq;
                matrix[p * n + q] = 0.0;
                matrix[q * n + p] = 0.0;
                for row in 0..n {
                    if row != p && row != q {
                        let arp = matrix[row * n + p];
                        let arq = matrix[row * n + q];
                        let new_p = cosine * arp - sine * arq;
                        let new_q = sine * arp + cosine * arq;
                        matrix[row * n + p] = new_p;
                        matrix[p * n + row] = new_p;
                        matrix[row * n + q] = new_q;
                        matrix[q * n + row] = new_q;
                    }
                    let vrp = vectors[row * n + p];
                    let vrq = vectors[row * n + q];
                    vectors[row * n + p] = cosine * vrp - sine * vrq;
                    vectors[row * n + q] = sine * vrp + cosine * vrq;
                }
            }
        }
    }
    Err(fit_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(key: &str, text: &str) -> Document {
        Document {
            key: key.into(),
            title: String::new(),
            text: text.into(),
        }
    }

    fn prior() -> Vec<String> {
        [
            "harbor pier vessel",
            "ship vessel harbor",
            "library book scroll",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn symmetric_solver_reconstructs_matrix_with_orthonormal_vectors() {
        let source = vec![4.0, 1.0, 2.0, 1.0, 3.0, 0.0, 2.0, 0.0, 5.0];
        let (values, vectors) = jacobi(source.clone(), 3, 32).unwrap();
        for row in 0..3 {
            for column in 0..3 {
                let reconstructed = (0..3)
                    .map(|k| vectors[row * 3 + k] * values[k] * vectors[column * 3 + k])
                    .sum::<f64>();
                assert!((source[row * 3 + column] - reconstructed).abs() < 1e-10);
                let inner = (0..3)
                    .map(|k| vectors[k * 3 + row] * vectors[k * 3 + column])
                    .sum::<f64>();
                assert!((inner - if row == column { 1.0 } else { 0.0 }).abs() < 1e-10);
            }
        }
        assert_eq!((values, vectors), jacobi(source, 3, 32).unwrap());
    }

    #[test]
    fn solver_refuses_unconverged_and_nonfinite_matrices() {
        assert!(jacobi(vec![2.0, 1.0, 1.0, 2.0], 2, 0).is_err());
        assert!(jacobi(vec![f64::NAN], 1, 32).is_err());
        assert_eq!(
            jacobi(vec![2.0, 0.0, 0.0, 2.0], 2, 0).unwrap().0,
            vec![2.0, 2.0]
        );
    }

    #[test]
    fn native_tokenization_is_unicode_lowercase_without_normalization() {
        let values = features("ÉCOLE école; e\u{301}cole / rock'n-roll under_score the and");
        assert_eq!(values["école"], 2);
        assert_eq!(values["e"], 1);
        assert_eq!(values["cole"], 1);
        assert_eq!(values["rock'n-roll"], 1);
        assert_eq!(values["under_score"], 1);
        assert_eq!(values["école::école"], 1);
        assert!(!values.contains_key("the"));
        assert_ne!(features("Straße"), features("STRASSE"));
    }

    #[test]
    fn actual_lsa_recalls_a_nonliteral_target_and_keeps_zero_candidates() {
        let documents = [document("pier", "pier"), document("unknown", "qzxwv")];
        let ranked = rank("ship", &documents, &prior(), Mode::Semantic, 10, false).unwrap();
        assert_eq!(ranked.dimensions, 2);
        assert_eq!(ranked.total, 2);
        assert_eq!(ranked.hits[0].index, 0);
        assert!(ranked.hits[0].semantic_score.unwrap() > 0.9);
        assert_eq!(ranked.hits[1].semantic_score, Some(0.0));
        assert!(!documents[0].text.contains("ship"));
        assert!(
            rank("qzxwv", &documents, &prior(), Mode::Semantic, 10, false)
                .unwrap()
                .hits
                .is_empty()
        );
    }

    #[test]
    fn negative_cosine_candidates_are_ranked_without_a_relevance_threshold() {
        let training = ["alpha beta", "beta gamma", "gamma delta"].map(str::to_owned);
        let documents = [document("alpha", "alpha"), document("delta", "delta")];
        let ranked = rank("alpha", &documents, &training, Mode::Semantic, 10, false).unwrap();
        assert_eq!(ranked.total, 2);
        assert_eq!(ranked.hits[1].index, 1);
        assert!(ranked.hits[1].score < -0.1);
    }

    #[test]
    fn targets_and_queries_never_change_the_training_basis() {
        let one = [document("pier", "pier")];
        let two = [
            document("pier", "pier"),
            document("extra", "nevertrained private additions"),
        ];
        let before = rank("ship", &one, &prior(), Mode::Semantic, 10, false).unwrap();
        let after = rank("ship", &two, &prior(), Mode::Semantic, 10, false).unwrap();
        assert_eq!(before.dimensions, after.dimensions);
        assert_eq!(before.hits[0].semantic_score, after.hits[0].semantic_score);
        assert!(
            rank("nevertrained", &two, &prior(), Mode::Semantic, 10, false)
                .unwrap()
                .hits
                .is_empty()
        );
    }

    #[test]
    fn hybrid_fuses_independent_lanes_and_limit_never_changes_scores() {
        let documents = [
            document("pier", "pier"),
            document("ship", "ship"),
            document("unknown", "qzxwv"),
        ];
        let all = rank("ship", &documents, &prior(), Mode::Hybrid, 10, false).unwrap();
        let small = rank("ship", &documents, &prior(), Mode::Hybrid, 1, false).unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(small.hits[0].index, all.hits[0].index);
        assert_eq!(small.hits[0].score, all.hits[0].score);
        let recalled = all.hits.iter().find(|hit| hit.index == 0).unwrap();
        assert!(recalled.lexical_rank.is_none());
        assert!(recalled.semantic_rank.is_some());
        let exact = all.hits.iter().find(|hit| hit.index == 1).unwrap();
        assert_eq!(exact.lexical_rank, Some(1));
        assert_eq!(
            exact.score,
            1.0 / 61.0 + 1.0 / (60.0 + exact.semantic_rank.unwrap() as f64)
        );
        let oov = rank("qzxwv", &documents, &prior(), Mode::Hybrid, 10, false).unwrap();
        assert_eq!(oov.total, 1);
        assert_eq!(oov.hits[0].lexical_rank, Some(1));
        assert!(oov.hits[0].semantic_rank.is_none());
    }

    #[test]
    fn equal_vectors_use_stable_keys_independent_of_input_order() {
        let documents = [document("b", "pier"), document("a", "pier")];
        let ranked = rank("ship", &documents, &prior(), Mode::Semantic, 10, false).unwrap();
        assert_eq!(
            ranked
                .hits
                .iter()
                .map(|hit| documents[hit.index].key.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        let reordered = [document("a", "pier"), document("b", "pier")];
        let ranked_again = rank("ship", &reordered, &prior(), Mode::Semantic, 10, false).unwrap();
        assert_eq!(ranked.hits[0].score, ranked_again.hits[0].score);
    }

    #[test]
    fn all_input_bounds_apply_before_oov_or_empty_result_shortcuts() {
        let duplicates = vec!["same training".to_owned(); 1025];
        assert!(rank("unseen", &[], &duplicates, Mode::Semantic, 1, false).is_err());
        let distinct = (0..513)
            .map(|index| format!("training {index}"))
            .collect::<Vec<_>>();
        assert!(rank("unseen", &[], &distinct, Mode::Semantic, 1, false).is_err());
        let oversized = vec!["x".repeat(MAX_BYTES)];
        assert!(rank("unseen", &[], &oversized, Mode::Semantic, 1, false).is_err());
        let long_token = vec!["É".repeat(2048)];
        assert!(
            rank("É", &[], &long_token, Mode::Semantic, 1, false)
                .unwrap()
                .hits
                .is_empty()
        );
        assert!(
            rank(
                "x",
                &[document("a", "x"), document("a", "y")],
                &prior(),
                Mode::Hybrid,
                1,
                false
            )
            .is_err()
        );
        assert!(checked_work(&[513, 8192, 192], MAX_PROJECTION_WORK).is_err());
    }

    #[test]
    fn tiny_tfidf_and_empty_training_have_explicit_behavior() {
        let training = vec!["quay".to_owned()];
        let documents = [document("a", "quay")];
        let result = rank("quay", &documents, &training, Mode::Semantic, 1, false).unwrap();
        assert_eq!(result.dimensions, 1);
        assert_eq!(result.hits[0].score, 1.0);
        assert!(rank("quay", &documents, &[], Mode::Hybrid, 1, false).is_err());
        let empty = rank("quay", &[], &[], Mode::Semantic, 1, false).unwrap();
        assert!(empty.hits.is_empty());
        assert_eq!(empty.dimensions, 0);
        let featureless = ["the and".to_owned(), "with our".to_owned()];
        let placeholder = [document("x", "__empty__")];
        let result = rank(
            "__empty__",
            &placeholder,
            &featureless,
            Mode::Semantic,
            10,
            false,
        )
        .unwrap();
        assert!(result.hits.is_empty());
        assert_eq!(result.dimensions, 1);
        let hybrid = rank(
            "__empty__",
            &placeholder,
            &featureless,
            Mode::Hybrid,
            10,
            false,
        )
        .unwrap();
        assert_eq!(hybrid.total, 1);
        assert_eq!(hybrid.hits[0].lexical_rank, Some(1));
        assert_eq!(hybrid.hits[0].semantic_rank, None);
        assert_eq!(hybrid.hits[0].semantic_score, None);
        assert_eq!(hybrid.hits[0].score, 1.0 / 61.0);
        // The spelling is not globally reserved: when source actually teaches
        // this token, it must receive the ordinary usable one-feature model.
        for training in [
            vec!["__empty__".to_owned()],
            vec!["__empty__".to_owned(), "the and".to_owned()],
        ] {
            for mode in [Mode::Semantic, Mode::Hybrid] {
                let learned = rank("__empty__", &placeholder, &training, mode, 10, false).unwrap();
                assert_eq!(learned.total, 1);
                assert_eq!(learned.hits[0].semantic_rank, Some(1));
                assert_eq!(learned.hits[0].semantic_score, Some(1.0));
            }
        }
    }
}
