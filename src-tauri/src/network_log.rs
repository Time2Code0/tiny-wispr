//! Every network request the app makes (only model downloads), kept in
//! `network-log.json` so the Privacy page can show them.

use serde::{Deserialize, Serialize};
use tw_core::Millis;
use tw_models::{RequestOutcome, RequestRecord};

use crate::state::Data;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestResult {
    Completed,
    Cancelled,
    Restarted,
    Failed,
}

/// One entry of `list_network_log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRequest {
    pub url: String,
    pub bytes: u64,
    pub started_at: Millis,
    pub finished_at: Millis,
    pub result: RequestResult,
    pub message: Option<String>,
}

impl From<&RequestRecord> for NetworkRequest {
    fn from(record: &RequestRecord) -> Self {
        let (result, message) = match &record.outcome {
            RequestOutcome::Completed => (RequestResult::Completed, None),
            RequestOutcome::Cancelled => (RequestResult::Cancelled, None),
            RequestOutcome::Restarted => (RequestResult::Restarted, None),
            RequestOutcome::Failed { message } => (RequestResult::Failed, Some(message.clone())),
        };
        Self {
            url: record.url.clone(),
            bytes: record.bytes,
            started_at: record.started,
            finished_at: record.finished,
            result,
            message,
        }
    }
}

pub fn record(data: &Data, record: &RequestRecord) {
    let entry = NetworkRequest::from(record);
    tracing::info!(url = %entry.url, bytes = entry.bytes, result = ?entry.result, "network request");
    if let Err(err) = data.network_log.update(|log| log.push(entry)) {
        tracing::warn!(%err, "could not save the network log");
    }
}
