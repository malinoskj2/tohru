"""Desktop observations of worker-show receipts, never task or speech authority."""
import json
import os
from pathlib import Path
import sqlite3
import time


class WorkerLedger:
    def __init__(self, directory):
        directory = Path(directory)
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.path = directory / "worker-receipts.sqlite3"
        self.connection = sqlite3.connect(self.path, timeout=2)
        os.chmod(self.path, 0o600)
        self.connection.execute("CREATE TABLE IF NOT EXISTS receipts(home TEXT, dispatch TEXT, request TEXT, observed REAL, body TEXT, PRIMARY KEY(home,dispatch))")

    def close(self):
        self.connection.close()

    def observe(self, home, dispatch, request, receipt, observed=None):
        if not isinstance(receipt, dict) or not isinstance(receipt.get("dispatch"), dict) or receipt["dispatch"].get("id") != dispatch:
            return False
        observed = time.time() if observed is None else observed
        with self.connection:
            self.connection.execute("INSERT INTO receipts VALUES(?,?,?,?,?) ON CONFLICT(home,dispatch) DO UPDATE SET request=excluded.request,observed=excluded.observed,body=excluded.body WHERE excluded.observed >= receipts.observed",
                                    (home, dispatch, request, observed, json.dumps(receipt, ensure_ascii=False)))
            self.connection.execute("DELETE FROM receipts WHERE observed < ?", (observed - 7 * 86400,))
        return True

    def snapshot(self, home, now=None):
        now = time.time() if now is None else now
        rows = self.connection.execute("SELECT dispatch,request,observed,body FROM receipts WHERE home=? ORDER BY observed DESC LIMIT 100", (home,))
        return [{"dispatch": dispatch, "observed_for_request": request, "observed_at": observed,
                 "age_seconds": max(0, now - observed), "stale": now - observed > 5,
                 "source": "orca orchestration worker-show", "receipt": json.loads(body)}
                for dispatch, request, observed, body in rows]
