//! Desktop observations of worker-show receipts, never task or speech authority.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::pyjson;

const RETENTION_SECONDS: f64 = 7.0 * 86400.0;
const STALE_SECONDS: f64 = 5.0;

pub struct WorkerLedger {
    connection: Connection,
}

/// One remembered receipt, newest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub dispatch: String,
    pub observed_for_request: String,
    pub observed_at: f64,
    pub age_seconds: f64,
    pub stale: bool,
    pub receipt: Value,
}

impl Observation {
    #[must_use]
    pub fn json(&self) -> Value {
        json!({"dispatch": self.dispatch, "observed_for_request": self.observed_for_request,
               "observed_at": self.observed_at, "age_seconds": age(self.age_seconds), "stale": self.stale,
               "source": "orca orchestration worker-show", "receipt": self.receipt})
    }
}

/// `max(0, age)`: Python keeps the integer zero when the observation is not in the past.
fn age(seconds: f64) -> Value {
    if seconds > 0.0 {
        json!(seconds)
    } else {
        json!(0)
    }
}

impl WorkerLedger {
    pub fn open(directory: &Path) -> rusqlite::Result<Self> {
        let _ = super::private_dir(directory);
        let path = directory.join("worker-receipts.sqlite3");
        let connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        connection.execute(
            "CREATE TABLE IF NOT EXISTS receipts(home TEXT, dispatch TEXT, request TEXT, observed REAL, body TEXT, PRIMARY KEY(home,dispatch))",
            [],
        )?;
        Ok(Self { connection })
    }

    /// Records `receipt` when it is for `dispatch`; false when it names another dispatch.
    pub fn observe(
        &self,
        home: &str,
        dispatch: &str,
        request: &str,
        receipt: &Value,
        observed: f64,
    ) -> rusqlite::Result<bool> {
        let named = receipt
            .get("dispatch")
            .filter(|value| value.is_object())
            .and_then(|value| value.get("id"));
        if named.and_then(Value::as_str) != Some(dispatch) {
            return Ok(false);
        }
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Deferred,
        )?;
        transaction.execute(
            "INSERT INTO receipts VALUES(?,?,?,?,?) ON CONFLICT(home,dispatch) DO UPDATE SET request=excluded.request,observed=excluded.observed,body=excluded.body WHERE excluded.observed >= receipts.observed",
            params![home, dispatch, request, observed, pyjson::dumps_unicode(receipt)],
        )?;
        transaction.execute(
            "DELETE FROM receipts WHERE observed < ?",
            [observed - RETENTION_SECONDS],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn snapshot(&self, home: &str, now: f64) -> rusqlite::Result<Vec<Observation>> {
        let mut statement = self
            .connection
            .prepare("SELECT dispatch,request,observed,body FROM receipts WHERE home=? ORDER BY observed DESC LIMIT 100")?;
        let rows = statement.query_map([home], |row| {
            let observed: f64 = row.get(2)?;
            let body: String = row.get(3)?;
            Ok(Observation {
                dispatch: row.get(0)?,
                observed_for_request: row.get(1)?,
                observed_at: observed,
                age_seconds: now - observed,
                stale: now - observed > STALE_SECONDS,
                receipt: serde_json::from_str(&body).unwrap_or(Value::Null),
            })
        })?;
        rows.collect()
    }
}
