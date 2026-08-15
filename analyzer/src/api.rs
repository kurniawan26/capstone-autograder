use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::capability;
use crate::notebook;
use crate::source::{self, LineIndex};
use crate::storage::Store;
use crate::types::*;

const DEFAULT_MAX_EVIDENCE: usize = 5;
const MAX_PARSE_FAILURES_REPORTED: usize = 20;
const MAX_UPLOAD_BYTES: usize = 128 * 1024 * 1024;

pub struct AppState {
    pub store: Store,
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/capabilities", get(capabilities))
        .route("/v1/analyze", post(analyze))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "capabilities": capability::CATALOG.len(),
    }))
}

async fn capabilities() -> impl IntoResponse {
    Json(serde_json::json!({ "capabilities": capability::CATALOG }))
}

fn fail(status: StatusCode, stage: &'static str, error: String) -> axum::response::Response {
    (status, Json(ErrorResponse { stage, error })).into_response()
}

async fn analyze(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AnalyzeRequest>,
) -> axum::response::Response {
    let started = Instant::now();

    if req.submission_id.trim().is_empty() {
        return fail(
            StatusCode::BAD_REQUEST,
            "validate",
            "submission_id is required".to_string(),
        );
    }
    let zip_bytes = match req.source_base64.as_deref() {
        Some(encoded) if !encoded.trim().is_empty() => {
            match BASE64.decode(encoded.trim().as_bytes()) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return fail(
                        StatusCode::BAD_REQUEST,
                        "decode_source",
                        format!("source_base64 bukan base64 yang sah: {e}"),
                    );
                }
            }
        }
        _ => {
            if req.source_key.trim().is_empty() {
                return fail(
                    StatusCode::BAD_REQUEST,
                    "validate",
                    "isi salah satu: source_key (ambil dari object storage) atau \
source_base64 (kirim ZIP langsung)"
                        .to_string(),
                );
            }
            match state.store.fetch(&req.source_key).await {
                Ok(bytes) => bytes,
                Err(e) => return fail(StatusCode::BAD_REQUEST, "fetch_source", e),
            }
        }
    };

    run_analysis(req, zip_bytes, started)
}

fn run_analysis(
    req: AnalyzeRequest,
    zip_bytes: Vec<u8>,
    started: Instant,
) -> axum::response::Response {
    let scan = match source::scan_zip(&zip_bytes) {
        Ok(scan) => scan,
        Err(e) => return fail(StatusCode::UNPROCESSABLE_ENTITY, "read_archive", e),
    };

    let max_evidence = req.max_evidence_per_check.unwrap_or(DEFAULT_MAX_EVIDENCE);

    let mut evidence_by_capability: BTreeMap<&'static str, Vec<Evidence>> = BTreeMap::new();
    let mut occurrences: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut parse_failures = Vec::new();

    for finding in capability::manifest::analyze(&scan.manifests, &scan.all_paths) {
        *occurrences.entry(finding.capability).or_insert(0) += 1;
        let bucket = evidence_by_capability
            .entry(finding.capability)
            .or_default();
        if bucket.len() < max_evidence {
            bucket.push(Evidence {
                file: finding.file,
                line: finding.line,
                column: 1,
                matched: finding.matched,
                snippet: finding.snippet,
            });
        }
    }

    for finding in capability::markup::analyze(&scan.markup) {
        *occurrences.entry(finding.capability).or_insert(0) += 1;
        let bucket = evidence_by_capability
            .entry(finding.capability)
            .or_default();
        if bucket.len() < max_evidence {
            bucket.push(Evidence {
                file: finding.file,
                line: finding.line,
                column: finding.column,
                matched: finding.matched,
                snippet: finding.snippet,
            });
        }
    }

    for finding in capability::artifact::analyze(&scan.all_paths) {
        *occurrences.entry(finding.capability).or_insert(0) += 1;
        let bucket = evidence_by_capability
            .entry(finding.capability)
            .or_default();
        if bucket.len() < max_evidence {
            bucket.push(Evidence {
                file: finding.file,
                line: 1,
                column: 1,
                matched: finding.matched,
                snippet: finding.snippet,
            });
        }
    }

    let record = |capability: &'static str,
                  file: String,
                  line: usize,
                  column: usize,
                  matched: String,
                  snippet: String,
                  occurrences_by: &mut BTreeMap<&'static str, usize>,
                  evidence_by: &mut BTreeMap<&'static str, Vec<Evidence>>| {
        *occurrences_by.entry(capability).or_insert(0) += 1;
        let bucket = evidence_by.entry(capability).or_default();
        if bucket.len() < max_evidence {
            bucket.push(Evidence {
                file,
                line,
                column,
                matched,
                snippet,
            });
        }
    };

    for file in &scan.python {
        for hit in capability::python::analyze(&file.content) {
            record(
                hit.capability,
                file.path.clone(),
                hit.line,
                hit.column,
                hit.matched,
                hit.snippet,
                &mut occurrences,
                &mut evidence_by_capability,
            );
        }
    }

    for file in &scan.notebooks {
        let parsed = match notebook::parse(&file.path, &file.content) {
            Some(parsed) => parsed,
            None => {
                if parse_failures.len() < MAX_PARSE_FAILURES_REPORTED {
                    parse_failures.push(ParseFailure {
                        file: file.path.clone(),
                        message: "notebook tidak dapat dibaca sebagai JSON .ipynb".to_string(),
                    });
                }
                continue;
            }
        };

        for cell in &parsed.code_cells {
            for hit in capability::python::analyze(&cell.source) {
                record(
                    hit.capability,
                    format!("{} (cell {})", parsed.path, cell.index),
                    hit.line,
                    hit.column,
                    hit.matched,
                    hit.snippet,
                    &mut occurrences,
                    &mut evidence_by_capability,
                );
            }
        }

        if parsed.markdown_cells > 0 {
            *occurrences.entry("notebook_narrative").or_insert(0) += parsed.markdown_cells;
            let bucket = evidence_by_capability
                .entry("notebook_narrative")
                .or_default();
            if bucket.len() < max_evidence {
                bucket.push(Evidence {
                    file: parsed.path.clone(),
                    line: 1,
                    column: 1,
                    matched: format!(
                        "{} sel markdown untuk {} sel kode",
                        parsed.markdown_cells,
                        parsed.code_cells.len()
                    ),
                    snippet: format!(
                        "{} memuat {} karakter penjelasan markdown",
                        parsed.path, parsed.markdown_chars
                    ),
                });
            }
        }
    }

    for file in &scan.files {
        let result = capability::js::analyze(&file.path, &file.content);

        if let Some(message) = result.parse_error {
            if parse_failures.len() < MAX_PARSE_FAILURES_REPORTED {
                parse_failures.push(ParseFailure {
                    file: file.path.clone(),
                    message,
                });
            }
        }

        if result.hits.is_empty() {
            continue;
        }

        let index = LineIndex::new(&file.content);
        for hit in result.hits {
            *occurrences.entry(hit.capability).or_insert(0) += 1;

            let bucket = evidence_by_capability.entry(hit.capability).or_default();
            if bucket.len() < max_evidence {
                let start = hit.span.start as usize;
                let (line, column) = index.locate(start);
                bucket.push(Evidence {
                    file: file.path.clone(),
                    line,
                    column,
                    matched: hit.matched,
                    snippet: source::snippet(&file.content, start, hit.span.end as usize),
                });
            }
        }
    }

    let checks = req
        .checks
        .iter()
        .map(|spec| {
            if !capability::known(&spec.capability) {
                return CheckResult {
                    id: spec.id.clone(),
                    title: spec.title.clone(),
                    track: spec.track.clone(),
                    capability: spec.capability.clone(),
                    expect: spec.expect,
                    status: Status::Inconclusive,
                    passed: None,
                    occurrences: 0,
                    evidence: Vec::new(),
                    note: Some(format!(
                        "capability '{}' tidak dikenal; lihat GET /v1/capabilities",
                        spec.capability
                    )),
                };
            }

            let count = occurrences
                .get(spec.capability.as_str())
                .copied()
                .unwrap_or(0);
            let evidence = evidence_by_capability
                .get(spec.capability.as_str())
                .cloned()
                .unwrap_or_default();

            let gap = if capability::needs_ast(&spec.capability) {
                (!scan.languages_unsupported.is_empty())
                    .then(|| scan.languages_unsupported.join(", "))
            } else {
                (!scan.manifests_unsupported.is_empty())
                    .then(|| scan.manifests_unsupported.join(", "))
            };

            let (status, note) = if count > 0 {
                let status = match spec.expect {
                    Expect::Present if count >= spec.min_occurrences.max(1) => Status::Passed,
                    Expect::Present => Status::Failed,
                    Expect::Absent => Status::Failed,
                };
                let note = match spec.expect {
                    Expect::Absent => Some(format!(
                        "ditemukan {count} penggunaan yang dilarang; lihat evidence"
                    )),
                    Expect::Present if status == Status::Failed => Some(format!(
                        "ditemukan {count}, rubrik meminta minimal {}",
                        spec.min_occurrences.max(1)
                    )),
                    Expect::Present => None,
                };
                (status, note)
            } else if scan.files.is_empty() && scan.manifests.is_empty() {
                (
                    Status::Inconclusive,
                    Some("tidak ada berkas yang dapat dianalisis di dalam arsip".to_string()),
                )
            } else if let Some(unanalysed) = gap {
                let note = if capability::needs_ast(&spec.capability) {
                    format!(
                        "tidak ditemukan pada berkas yang dianalisis, tetapi submission memuat \
kode {unanalysed} yang belum dapat dibaca analyzer — perlu diperiksa manual"
                    )
                } else {
                    format!(
                        "tidak ditemukan, tetapi submission memuat manifest {unanalysed} \
yang belum dapat dibaca analyzer — perlu diperiksa manual"
                    )
                };
                (Status::Inconclusive, Some(note))
            } else {
                let status = match spec.expect {
                    Expect::Present => Status::Failed,
                    Expect::Absent => Status::Passed,
                };
                (status, None)
            };

            CheckResult {
                id: spec.id.clone(),
                title: spec.title.clone(),
                track: spec.track.clone(),
                capability: spec.capability.clone(),
                expect: spec.expect,
                status,
                passed: status.as_bool(),
                occurrences: count,
                evidence,
                note,
            }
        })
        .collect();

    let detected = occurrences.keys().map(|id| id.to_string()).collect();

    let response = AnalyzeResponse {
        submission_id: req.submission_id,
        source: SourceInfo {
            files_scanned: scan.files.len()
                + scan.python.len()
                + scan.notebooks.len()
                + scan.markup.len()
                + scan.manifests.len(),
            code_files: scan.files.len(),
            python_files: scan.python.len(),
            notebook_files: scan.notebooks.len(),
            markup_files: scan.markup.len(),
            manifest_files: scan.manifests.len(),
            files_skipped: scan.skipped,
            bytes_scanned: scan.bytes,
            root_stripped: scan.root,
            parse_failures,
            truncated: scan.truncated,
            coverage: Coverage {
                languages_present: scan.languages_present.clone(),
                languages_analysed: source::ANALYSED_LANGUAGES
                    .iter()
                    .filter(|language| scan.languages_present.contains(&language.to_string()))
                    .map(|language| language.to_string())
                    .collect(),
                languages_unsupported: scan.languages_unsupported.clone(),
                manifests_read: scan.manifests_read.clone(),
                manifests_unsupported: scan.manifests_unsupported.clone(),
            },
        },
        checks,
        capabilities_detected: detected,
        duration_ms: started.elapsed().as_millis(),
    };

    (StatusCode::OK, Json(response)).into_response()
}
