from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from inference.modal_find_worker import (
    FindShard,
    _extract_frames,
    _merge_frame_hits,
    _optional_str,
    _resolve_model_id,
    _split_rawvideo_frames,
)


class ModalFindWorkerTests(unittest.TestCase):
    def test_merge_frame_hits_coalesces_neighboring_matches(self) -> None:
        hits = _merge_frame_hits(
            sample_id=42,
            scan_start_ms=0,
            scan_end_ms=10_000,
            sample_fps=1.0,
            frame_times_ms=[1_000, 2_000, 8_000],
            scores=[0.4, 0.6, 0.7],
        )

        self.assertEqual(len(hits), 2)
        self.assertEqual((hits[0].sample_id, hits[0].start_ms, hits[0].end_ms), (42, 500, 4_500))
        self.assertEqual((hits[1].sample_id, hits[1].start_ms, hits[1].end_ms), (42, 7_500, 10_000))

    def test_resolve_model_id_supports_siglip2_alias(self) -> None:
        self.assertEqual(_resolve_model_id("siglip2_base"), "google/siglip2-base-patch16-224")

    def test_split_rawvideo_frames_splits_evenly(self) -> None:
        frame_one = bytes([1, 2, 3] * 4)
        frame_two = bytes([4, 5, 6] * 4)
        frames = _split_rawvideo_frames(frame_one + frame_two, width=2, height=2)

        self.assertEqual(len(frames), 2)
        self.assertEqual(bytes(frames[0]), frame_one)
        self.assertEqual(bytes(frames[1]), frame_two)

    def test_optional_str_normalizes_blank_values(self) -> None:
        self.assertIsNone(_optional_str(None))
        self.assertIsNone(_optional_str("   "))
        self.assertEqual(_optional_str("  value  "), "value")

    def test_extract_frames_supports_image_inputs(self) -> None:
        from PIL import Image

        with tempfile.TemporaryDirectory() as tempdir:
            image_path = Path(tempdir) / "frame.jpg"
            Image.new("RGB", (32, 16), color=(255, 0, 0)).save(image_path)

            frame_times_ms, frames, decode_ms = _extract_frames(
                source_ref=str(image_path),
                shard=FindShard(
                    shard_id="shd-1",
                    job_id="job-1",
                    customer_id="cust-1",
                    lane="interactive",
                    priority=100,
                    attempt=0,
                    query_id="qry-1",
                    query_text="red frame",
                    source_uri=str(image_path),
                    asset_id="frame.jpg",
                    decode_hint=None,
                    sample_id=0,
                    scan_start_ms=0,
                    scan_end_ms=1,
                    overlap_ms=0,
                    sample_fps=1.0,
                    model="siglip2_base",
                    created_at_ms=1,
                ),
            )

        self.assertEqual(frame_times_ms, [0])
        self.assertEqual(len(frames), 1)
        self.assertGreaterEqual(decode_ms, 0)


if __name__ == "__main__":
    unittest.main()
