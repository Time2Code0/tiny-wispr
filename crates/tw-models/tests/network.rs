//! Real requests to Hugging Face. They run only with
//! `TINY_WISPR_NETWORK_TESTS=1`; downloads go to `TINY_WISPR_TEST_MODELS_DIR`
//! or the system temp dir.

#![allow(clippy::unwrap_used, reason = "test helpers outside #[test] functions")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tw_models::{DownloadRequest, Downloader, Inventory, ModelKind, RequestOutcome};

fn enabled() -> bool {
    let on = std::env::var("TINY_WISPR_NETWORK_TESTS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipped: set TINY_WISPR_NETWORK_TESTS=1 to run");
    }
    on
}

/// Hugging Face answers a pinned LFS file with a redirect that carries the
/// file's size and sha256, so nothing large is downloaded.
#[test]
fn every_catalog_url_answers_with_the_recorded_size_and_hash() {
    if !enabled() {
        return;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    let mut problems = Vec::new();
    for model in tw_core::models::catalog() {
        let url = model.url.clone().unwrap();
        let response = agent.head(&url).call().unwrap();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.trim_matches('"').to_owned())
        };
        let size = header("x-linked-size").and_then(|v| v.parse::<u64>().ok());
        let sha = header("x-linked-etag");
        let status = response.status().as_u16();
        let ok = status == 302
            && size == Some(model.size_bytes)
            && sha.as_deref() == model.sha256.as_deref();
        println!(
            "{:<40} {status} {size:?} {}",
            model.id,
            if ok { "ok" } else { "MISMATCH" }
        );
        if !ok {
            problems.push(format!("{}: {status} size {size:?} sha {sha:?}", model.id));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn downloads_a_real_model_with_an_interruption() {
    if !enabled() {
        return;
    }
    let model = tw_core::models::catalog()
        .into_iter()
        .filter(|m| m.kind == ModelKind::Voice)
        .min_by_key(|m| m.size_bytes)
        .unwrap();
    let root = std::env::var_os("TINY_WISPR_TEST_MODELS_DIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("tw-models-network-test");
    let _ = std::fs::remove_dir_all(&root);
    let inventory = Inventory::new(&root);
    let request = DownloadRequest::for_model(&model).unwrap();
    let records = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&records);
    let downloader =
        Downloader::new().with_request_log(move |r| sink.lock().unwrap().push(r.clone()));

    let cancel = AtomicBool::new(false);
    let interrupted = downloader.download(
        &inventory,
        &request,
        &mut |p| {
            if p.received >= model.size_bytes / 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(
        matches!(interrupted, Err(tw_models::ModelsError::Cancelled)),
        "{interrupted:?}"
    );

    let path = downloader
        .download(&inventory, &request, &mut |_| {}, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().len(), model.size_bytes);
    let records = records.lock().unwrap();
    for r in records.iter() {
        println!(
            "{} {:?} {} bytes in {} ms",
            r.url,
            r.outcome,
            r.bytes,
            r.finished - r.started
        );
    }
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].outcome, RequestOutcome::Cancelled);
    assert_eq!(records[1].outcome, RequestOutcome::Completed);
    assert_eq!(
        records[0].bytes + records[1].bytes,
        model.size_bytes,
        "the second request continued where the first stopped"
    );
    let _ = std::fs::remove_dir_all(&root);
}
