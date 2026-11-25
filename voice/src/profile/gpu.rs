use serde_json::Value;
use std::{process::Command, time::Duration};

const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const STATUS_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(super) enum TelemetryError {
    #[error("gpu-status is not installed, so the shared GPU scheduler is unavailable")]
    Missing,
    #[error("gpu-status failed")]
    Failed(#[source] anyhow::Error),
    #[error("gpu-status returned invalid JSON")]
    Json(#[from] serde_json::Error),
    #[error("GPU telemetry unavailable: {0}")]
    Unavailable(String),
}

/// Committed VRAM: measured use, with each running reservation charged at its full budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Usage {
    pub percent: f64,
    pub committed_mib: u64,
    pub total_mib: u64,
}

/// From `gpu-status --json`. Desktop headroom is a reserve, not use, so it is left out; an
/// exclusive reservation commits the whole GPU.
pub(super) fn usage(status: &Value) -> Result<Usage, TelemetryError> {
    if let Some(error) = status["telemetry_error"].as_str().filter(|e| !e.is_empty()) {
        return Err(TelemetryError::Unavailable(error.to_owned()));
    }
    let memory = &status["memory"];
    let (Some(total), Some(used)) = (memory["total_mib"].as_u64(), memory["used_mib"].as_u64())
    else {
        return Err(TelemetryError::Unavailable("no memory totals".into()));
    };
    if total == 0 {
        return Err(TelemetryError::Unavailable(
            "GPU telemetry reports no memory".into(),
        ));
    }
    let attributed = memory["attributed_mib"].as_u64().unwrap_or(0);
    let running = status["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|job| job["state"] == "running");
    let mut reserved = 0;
    for job in running {
        if job["mode"] == "exclusive" {
            return Ok(Usage {
                percent: 100.,
                committed_mib: total,
                total_mib: total,
            });
        }
        reserved += job["vram_mib"].as_u64().unwrap_or(0);
    }
    let committed = used.max((used + reserved).saturating_sub(attributed));
    Ok(Usage {
        percent: 100. * committed as f64 / total as f64,
        committed_mib: committed,
        total_mib: total,
    })
}

pub(super) fn status() -> Result<Usage, TelemetryError> {
    if !crate::process::installed("gpu-status") {
        return Err(TelemetryError::Missing);
    }
    let bytes = crate::process::output(
        Command::new("gpu-status").arg("--json"),
        b"",
        STATUS_TIMEOUT,
        STATUS_LIMIT,
    )
    .map_err(TelemetryError::Failed)?;
    usage(&serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn status(used: u64, jobs: Value, attributed: u64) -> Value {
        json!({"memory":{"total_mib":32000,"used_mib":used,"headroom_mib":4096,"attributed_mib":attributed},"telemetry_error":null,"jobs":jobs})
    }

    #[test]
    fn running_reservations_count_at_their_full_budget() {
        let usage = usage(&status(
            4000,
            json!([{"state":"running","mode":"shared","vram_mib":12000},{"state":"waiting","mode":"shared","vram_mib":20000}]),
            1000,
        ))
        .unwrap();
        assert_eq!((usage.committed_mib, usage.total_mib), (15000, 32000));
        assert!((usage.percent - 46.875).abs() < 1e-9);
    }

    #[test]
    fn measured_use_wins_when_higher_and_exclusive_means_full() {
        assert_eq!(
            usage(&status(20000, json!([]), 0)).unwrap().committed_mib,
            20000
        );
        let exclusive = json!([{"state":"running","mode":"exclusive","vram_mib":0}]);
        assert!((usage(&status(2000, exclusive, 0)).unwrap().percent - 100.).abs() < 1e-9);
        let mut failed = status(2000, json!([]), 0);
        failed["telemetry_error"] = "nvidia-smi failed".into();
        assert!(matches!(
            usage(&failed),
            Err(TelemetryError::Unavailable(_))
        ));
    }
}
