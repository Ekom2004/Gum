from __future__ import annotations

import sys
import unittest
from pathlib import Path

if sys.version_info < (3, 10):
    raise unittest.SkipTest("gum sdk tests require Python 3.10+")

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "sdk"))

import gum
from gum.client import BackfillRef, RunRef


class _FakeClient:
    def __init__(self) -> None:
        self.enqueued: list[tuple[str, dict[str, object], str | None]] = []
        self.backfills: list[tuple[str, list[dict[str, object]]]] = []

    def enqueue(self, job_id: str, payload: dict[str, object], *, delay: str | None = None) -> RunRef:
        self.enqueued.append((job_id, payload, delay))
        return RunRef(id="run_123", status="queued")

    def backfill(self, job_id: str, items: list[dict[str, object]]) -> BackfillRef:
        self.backfills.append((job_id, items))
        return BackfillRef(id="bf_123", status="queued", enqueued=len(items))


class GumJobTests(unittest.TestCase):
    def test_decorator_wraps_function_and_exposes_policy(self) -> None:
        resend_limit = gum.rate_limit("500/h")

        @gum.job(
            every="20d",
            retries=5,
            timeout="5m",
            rate_limit=resend_limit,
            concurrency=20,
            cpu=2,
            memory="2gb",
            key="event_id",
        )
        def send_followup() -> str:
            return "ok"

        self.assertEqual(send_followup(), "ok")
        self.assertEqual(send_followup.name, "send_followup")
        self.assertEqual(send_followup.id, "job_send_followup")
        self.assertEqual(send_followup.policy.id, "job_send_followup")
        self.assertEqual(send_followup.policy.every, "20d")
        self.assertEqual(send_followup.policy.retries, 5)
        self.assertEqual(send_followup.policy.timeout, "5m")
        self.assertEqual(send_followup.policy.rate_limit, "500/h")
        self.assertEqual(send_followup.policy.concurrency, 20)
        self.assertEqual(send_followup.policy.cpu, 2)
        self.assertEqual(send_followup.policy.memory, "2gb")
        self.assertEqual(send_followup.policy.key, "event_id")

    def test_enqueue_uses_keyword_payload_and_client(self) -> None:
        client = _FakeClient()

        @gum.job(retries=8, timeout="15m", client=client)
        def sync_customer(customer_id: str) -> None:
            raise AssertionError("job body should not run during enqueue")

        run = sync_customer.enqueue(customer_id="cus_123")

        self.assertEqual(run.id, "run_123")
        self.assertFalse(run.deduped)
        self.assertEqual(client.enqueued, [("job_sync_customer", {"customer_id": "cus_123"}, None)])

    def test_enqueue_supports_delay(self) -> None:
        client = _FakeClient()

        @gum.job(client=client)
        def send_reminder(user_id: str) -> None:
            raise AssertionError("job body should not run during enqueue")

        run = send_reminder.enqueue(user_id="usr_1", delay="10m")

        self.assertEqual(run.id, "run_123")
        self.assertEqual(
            client.enqueued,
            [("job_send_reminder", {"user_id": "usr_1"}, "10m")],
        )

    def test_backfill_passes_items_through(self) -> None:
        client = _FakeClient()

        @gum.job(client=client)
        def summarize_session(session_id: str) -> None:
            raise AssertionError("job body should not run during backfill")

        backfill = summarize_session.backfill(
            [
                {"session_id": "sess_1"},
                {"session_id": "sess_2"},
            ]
        )

        self.assertEqual(backfill.id, "bf_123")
        self.assertEqual(backfill.enqueued, 2)
        self.assertEqual(
            client.backfills,
            [("job_summarize_session", [{"session_id": "sess_1"}, {"session_id": "sess_2"}])],
        )


if __name__ == "__main__":
    unittest.main()
