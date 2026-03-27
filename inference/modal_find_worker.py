from __future__ import annotations

import os
import subprocess
import threading
from io import BytesIO
from collections import OrderedDict
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from pathlib import Path
from time import monotonic
from typing import Any
from urllib.parse import urlsplit
from urllib.request import urlopen

try:
    import modal
except ImportError:  # pragma: no cover - local unit tests may run without Modal installed
    modal = None

_APP_NAME = os.getenv("MX8_FIND_MODAL_APP_NAME", "mx8-find-worker")
_MODEL_CACHE_DIR = Path("/models/hf")
_MODEL_CACHE_VOLUME_NAME = os.getenv("MX8_FIND_MODAL_HF_CACHE_VOLUME", "mx8-find-hf-cache")

if modal is not None:
    _model_cache_volume = modal.Volume.from_name(_MODEL_CACHE_VOLUME_NAME, create_if_missing=True)
    app = modal.App(_APP_NAME)
    image = (
        modal.Image.debian_slim(python_version="3.11")
        .apt_install("ffmpeg")
        .pip_install(
            "torch>=2.7,<3.0",
            "transformers>=4.57,<5.0",
            "pillow>=10,<12",
            "safetensors>=0.4,<1.0",
        )
    )
else:  # pragma: no cover - local unit tests may run without Modal installed
    _model_cache_volume = None
    app = None
    image = None


@dataclass(frozen=True)
class MatchSegment:
    sample_id: int
    start_ms: int
    end_ms: int

    def validate(self) -> None:
        if self.sample_id < 0:
            raise ValueError("sample_id must be >= 0")
        if self.start_ms < 0:
            raise ValueError("start_ms must be >= 0")
        if self.end_ms <= self.start_ms:
            raise ValueError("end_ms must be > start_ms")


@dataclass(frozen=True)
class FindShard:
    shard_id: str
    job_id: str
    customer_id: str
    lane: str
    priority: int
    attempt: int
    query_id: str
    query_text: str
    source_uri: str
    asset_id: str
    decode_hint: str | None
    sample_id: int
    scan_start_ms: int
    scan_end_ms: int
    overlap_ms: int
    sample_fps: float
    model: str
    created_at_ms: int
    source_access_url: str | None = None

    def validate(self) -> None:
        if not self.shard_id.strip():
            raise ValueError("shard_id must be non-empty")
        if not self.job_id.strip():
            raise ValueError("job_id must be non-empty")
        if not self.customer_id.strip():
            raise ValueError("customer_id must be non-empty")
        if not self.query_id.strip():
            raise ValueError("query_id must be non-empty")
        if not self.query_text.strip():
            raise ValueError("query_text must be non-empty")
        if not self.source_uri.strip():
            raise ValueError("source_uri must be non-empty")
        if not self.asset_id.strip():
            raise ValueError("asset_id must be non-empty")
        if self.source_access_url is not None and not self.source_access_url.strip():
            raise ValueError("source_access_url must be non-empty when set")
        if self.sample_id < 0:
            raise ValueError("sample_id must be >= 0")
        if self.scan_start_ms < 0:
            raise ValueError("scan_start_ms must be >= 0")
        if self.scan_end_ms <= self.scan_start_ms:
            raise ValueError("scan_end_ms must be > scan_start_ms")
        if self.overlap_ms < 0:
            raise ValueError("overlap_ms must be >= 0")
        if self.sample_fps <= 0:
            raise ValueError("sample_fps must be > 0")
        if not self.model.strip():
            raise ValueError("model must be non-empty")


@dataclass(frozen=True)
class FindShardStats:
    sampled_frames: int
    decode_ms: int
    inference_ms: int
    wall_ms: int

    def validate(self) -> None:
        if self.sampled_frames < 0:
            raise ValueError("sampled_frames must be >= 0")
        if self.decode_ms < 0:
            raise ValueError("decode_ms must be >= 0")
        if self.inference_ms < 0:
            raise ValueError("inference_ms must be >= 0")
        if self.wall_ms < 0:
            raise ValueError("wall_ms must be >= 0")


@dataclass(frozen=True)
class FindShardResult:
    shard_id: str
    job_id: str
    customer_id: str
    asset_id: str
    status: str
    hits: tuple[MatchSegment, ...]
    stats: FindShardStats
    error: str | None = None

    def validate(self) -> None:
        if not self.shard_id.strip():
            raise ValueError("shard_id must be non-empty")
        if not self.job_id.strip():
            raise ValueError("job_id must be non-empty")
        if not self.customer_id.strip():
            raise ValueError("customer_id must be non-empty")
        if not self.asset_id.strip():
            raise ValueError("asset_id must be non-empty")
        if self.status not in {"ok", "error"}:
            raise ValueError(f"unsupported shard result status: {self.status!r}")
        if self.status == "ok" and self.error is not None:
            raise ValueError("successful shard result must not include error")
        if self.status == "error" and not (self.error or "").strip():
            raise ValueError("error shard result must include error")
        self.stats.validate()
        for hit in self.hits:
            hit.validate()


def find_shard_from_payload(payload: dict[str, object]) -> FindShard:
    shard = FindShard(
        shard_id=str(payload["shard_id"]),
        job_id=str(payload["job_id"]),
        customer_id=str(payload["customer_id"]),
        lane=str(payload["lane"]),
        priority=int(payload["priority"]),
        attempt=int(payload["attempt"]),
        query_id=str(payload["query_id"]),
        query_text=str(payload["query_text"]),
        source_uri=str(payload["source_uri"]),
        asset_id=str(payload["asset_id"]),
        decode_hint=_optional_str(payload.get("decode_hint")),
        sample_id=int(payload["sample_id"]),
        scan_start_ms=int(payload["scan_start_ms"]),
        scan_end_ms=int(payload["scan_end_ms"]),
        overlap_ms=int(payload["overlap_ms"]),
        sample_fps=float(payload["sample_fps"]),
        model=str(payload["model"]),
        created_at_ms=int(payload["created_at_ms"]),
        source_access_url=_optional_str(payload.get("source_access_url")),
    )
    shard.validate()
    return shard


def find_shard_result_to_payload(result: FindShardResult) -> dict[str, object]:
    result.validate()
    return {
        "shard_id": result.shard_id,
        "job_id": result.job_id,
        "customer_id": result.customer_id,
        "asset_id": result.asset_id,
        "status": result.status,
        "hits": [
            {
                "sample_id": hit.sample_id,
                "start_ms": hit.start_ms,
                "end_ms": hit.end_ms,
            }
            for hit in result.hits
        ],
        "stats": {
            "sampled_frames": result.stats.sampled_frames,
            "decode_ms": result.stats.decode_ms,
            "inference_ms": result.stats.inference_ms,
            "wall_ms": result.stats.wall_ms,
        },
        "error": result.error,
    }


def _optional_str(value: object) -> str | None:
    if value is None:
        return None
    normalized = str(value).strip()
    return normalized or None


@dataclass
class _ModelRuntime:
    processor: Any
    model: Any
    device: str
    torch_dtype: Any


_runtime_lock = threading.Lock()
_runtime_by_model: dict[str, _ModelRuntime] = {}
_query_cache_lock = threading.Lock()
_query_cache: OrderedDict[tuple[str, str], tuple[float, Any]] = OrderedDict()


def process_shard_payload(payload: dict[str, object]) -> dict[str, object]:
    shard = find_shard_from_payload(payload)
    result = _process_shard(shard)
    return find_shard_result_to_payload(result)


if modal is not None:

    @app.function(
        image=image,
        gpu=os.getenv("MX8_FIND_MODAL_GPU", "L4"),
        timeout=int(os.getenv("MX8_FIND_MODAL_TIMEOUT_SECS", "3600")),
        min_containers=int(os.getenv("MX8_FIND_MODAL_MIN_CONTAINERS", "0")),
        buffer_containers=int(os.getenv("MX8_FIND_MODAL_BUFFER_CONTAINERS", "0")),
        scaledown_window=int(os.getenv("MX8_FIND_MODAL_SCALEDOWN_WINDOW_SECS", "60")),
        volumes={"/models": _model_cache_volume},
    )
    def process_shard(payload: dict[str, object]) -> dict[str, object]:
        return process_shard_payload(payload)


def _process_shard(shard: FindShard) -> FindShardResult:
    started = monotonic()
    sampled_frames = 0
    decode_ms = 0
    inference_ms = 0
    source_ref = shard.source_access_url or shard.source_uri
    try:
        frame_times_ms, frames, decode_ms = _extract_frames(
            source_ref=source_ref,
            shard=shard,
        )
        sampled_frames = len(frames)
        if not frames:
            hits: tuple[MatchSegment, ...] = ()
        else:
            scores, inference_ms = _score_frames(
                shard=shard,
                frames=frames,
                source_ref=source_ref,
            )
            hits = _merge_frame_hits(
                sample_id=shard.sample_id,
                scan_start_ms=shard.scan_start_ms,
                scan_end_ms=shard.scan_end_ms,
                sample_fps=shard.sample_fps,
                frame_times_ms=frame_times_ms,
                scores=scores,
            )
        result = FindShardResult(
            shard_id=shard.shard_id,
            job_id=shard.job_id,
            customer_id=shard.customer_id,
            asset_id=shard.asset_id,
            status="ok",
            hits=hits,
            stats=FindShardStats(
                sampled_frames=sampled_frames,
                decode_ms=decode_ms,
                inference_ms=inference_ms,
                wall_ms=max(0, int((monotonic() - started) * 1000)),
            ),
        )
        result.validate()
        return result
    except Exception as exc:
        result = FindShardResult(
            shard_id=shard.shard_id,
            job_id=shard.job_id,
            customer_id=shard.customer_id,
            asset_id=shard.asset_id,
            status="error",
            hits=(),
            stats=FindShardStats(
                sampled_frames=sampled_frames,
                decode_ms=decode_ms,
                inference_ms=inference_ms,
                wall_ms=max(0, int((monotonic() - started) * 1000)),
            ),
            error=str(exc),
        )
        result.validate()
        return result


def _extract_frames(*, source_ref: str, shard: FindShard) -> tuple[list[int], list[memoryview], int]:
    if _is_image_shard(shard):
        return _extract_image_frame(source_ref=source_ref, shard=shard)
    started = monotonic()
    scan_duration_secs = max(0.001, (shard.scan_end_ms - shard.scan_start_ms) / 1000.0)
    frame_width = _frame_size()
    frame_height = _frame_size()
    command = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-ss",
        f"{shard.scan_start_ms / 1000.0:.3f}",
        "-t",
        f"{scan_duration_secs:.3f}",
        "-i",
        source_ref,
        "-vf",
        (
            f"fps={shard.sample_fps},"
            f"scale={frame_width}:{frame_height}:force_original_aspect_ratio=decrease,"
            f"pad={frame_width}:{frame_height}:(ow-iw)/2:(oh-ih)/2:black"
        ),
        "-pix_fmt",
        "rgb24",
        "-vsync",
        "vfr",
        "-f",
        "rawvideo",
        "pipe:1",
    ]
    completed = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        stderr = (completed.stderr or completed.stdout or b"").decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"ffmpeg frame extraction failed for {shard.asset_id}: {stderr}")
    frames = _split_rawvideo_frames(completed.stdout or b"", width=frame_width, height=frame_height)
    if not frames:
        return [], [], max(0, int((monotonic() - started) * 1000))
    step_ms = _frame_step_ms(shard.sample_fps)
    frame_times_ms = [
        min(shard.scan_end_ms - 1, shard.scan_start_ms + (index * step_ms))
        for index in range(len(frames))
    ]
    return frame_times_ms, frames, max(0, int((monotonic() - started) * 1000))


def _extract_image_frame(*, source_ref: str, shard: FindShard) -> tuple[list[int], list[memoryview], int]:
    started = monotonic()
    from PIL import Image

    image_bytes = _read_source_bytes(source_ref)
    with Image.open(BytesIO(image_bytes)) as image:
        prepared = _prepare_image(image)
    return [0], [memoryview(prepared)], max(0, int((monotonic() - started) * 1000))


def _score_frames(*, shard: FindShard, frames: Sequence[memoryview], source_ref: str) -> tuple[list[float], int]:
    started = monotonic()
    runtime = _runtime_for_model(shard.model)
    query_embedding = _cached_query_embedding(runtime=runtime, shard=shard).to(runtime.device)

    import torch
    from PIL import Image

    scores: list[float] = []
    batch_size = _batch_size()
    frame_size = _frame_size()
    for batch_frames in _batched(frames, batch_size):
        images = []
        for frame in batch_frames:
            images.append(Image.frombytes("RGB", (frame_size, frame_size), bytes(frame)))
        inputs = runtime.processor(images=images, return_tensors="pt")
        inputs = {name: tensor.to(runtime.device) for name, tensor in inputs.items()}
        with torch.inference_mode():
            image_features = runtime.model.get_image_features(**inputs)
        image_features = image_features.float()
        image_features = image_features / image_features.norm(p=2, dim=-1, keepdim=True)
        batch_scores = torch.matmul(image_features, query_embedding.unsqueeze(1)).squeeze(1)
        scores.extend(float(score) for score in batch_scores.detach().cpu().tolist())
    return scores, max(0, int((monotonic() - started) * 1000))


def _runtime_for_model(model_alias: str) -> _ModelRuntime:
    model_id = _resolve_model_id(model_alias)
    with _runtime_lock:
        runtime = _runtime_by_model.get(model_id)
        if runtime is not None:
            return runtime

        import torch
        from transformers import AutoModel, AutoProcessor

        os.environ.setdefault("HF_HOME", str(_MODEL_CACHE_DIR))
        _MODEL_CACHE_DIR.mkdir(parents=True, exist_ok=True)
        device = "cuda" if torch.cuda.is_available() else "cpu"
        torch_dtype = torch.float16 if device == "cuda" else torch.float32
        processor = AutoProcessor.from_pretrained(model_id)
        model = AutoModel.from_pretrained(
            model_id,
            torch_dtype=torch_dtype,
            attn_implementation="sdpa",
        ).to(device)
        model.eval()
        runtime = _ModelRuntime(
            processor=processor,
            model=model,
            device=device,
            torch_dtype=torch_dtype,
        )
        _runtime_by_model[model_id] = runtime
        if _model_cache_volume is not None:  # pragma: no branch - only true on Modal
            try:
                _model_cache_volume.commit()
            except Exception:
                pass
        return runtime


def _cached_query_embedding(*, runtime: _ModelRuntime, shard: FindShard):
    query_key = (_resolve_model_id(shard.model), shard.query_id)
    now = monotonic()
    with _query_cache_lock:
        _prune_query_cache_locked(now=now)
        cached = _query_cache.get(query_key)
        if cached is not None:
            _query_cache.move_to_end(query_key)
            return cached[1]

    import torch

    inputs = runtime.processor(text=[shard.query_text], return_tensors="pt")
    inputs = {name: tensor.to(runtime.device) for name, tensor in inputs.items()}
    with torch.inference_mode():
        text_features = runtime.model.get_text_features(**inputs)
    text_features = text_features.float()
    text_features = text_features / text_features.norm(p=2, dim=-1, keepdim=True)
    embedding = text_features[0].detach().cpu()

    with _query_cache_lock:
        _query_cache[query_key] = (now, embedding)
        _query_cache.move_to_end(query_key)
        while len(_query_cache) > _query_cache_max_entries():
            _query_cache.popitem(last=False)
    return embedding


def _prune_query_cache_locked(*, now: float) -> None:
    ttl_secs = _query_cache_ttl_secs()
    expired = [
        query_key
        for query_key, (inserted_at, _) in _query_cache.items()
        if now - inserted_at > ttl_secs
    ]
    for query_key in expired:
        _query_cache.pop(query_key, None)


def _merge_frame_hits(
    *,
    sample_id: int,
    scan_start_ms: int,
    scan_end_ms: int,
    sample_fps: float,
    frame_times_ms: Sequence[int],
    scores: Sequence[float],
) -> tuple[MatchSegment, ...]:
    threshold = _score_threshold()
    pad_before_ms = _pad_before_ms()
    pad_after_ms = _pad_after_ms()
    merge_gap_ms = _merge_gap_ms()
    frame_step_ms = _frame_step_ms(sample_fps)

    merged: list[MatchSegment] = []
    current: MatchSegment | None = None
    for frame_time_ms, score in zip(frame_times_ms, scores):
        if score < threshold:
            continue
        start_ms = max(scan_start_ms, frame_time_ms - pad_before_ms)
        end_ms = min(scan_end_ms, frame_time_ms + frame_step_ms + pad_after_ms)
        if end_ms <= start_ms:
            end_ms = min(scan_end_ms, start_ms + 1)
        if current is not None and start_ms <= current.end_ms + merge_gap_ms:
            current = MatchSegment(
                sample_id=sample_id,
                start_ms=current.start_ms,
                end_ms=max(current.end_ms, end_ms),
            )
            merged[-1] = current
            continue
        current = MatchSegment(sample_id=sample_id, start_ms=start_ms, end_ms=end_ms)
        merged.append(current)
    return tuple(merged)


def _is_image_shard(shard: FindShard) -> bool:
    hint = (shard.decode_hint or "").strip().lower()
    if hint.startswith("mx8:vision:imagefolder;") or hint.startswith("mx8:image;"):
        return True
    path = urlsplit(shard.source_uri).path.lower()
    return any(path.endswith(extension) for extension in IMAGE_EXTENSIONS)


IMAGE_EXTENSIONS = (
    ".bmp",
    ".gif",
    ".jpeg",
    ".jpg",
    ".png",
    ".webp",
)


def _read_source_bytes(source_ref: str) -> bytes:
    parsed = urlsplit(source_ref)
    if parsed.scheme in {"http", "https", "file"}:
        with urlopen(source_ref) as response:
            return response.read()
    return Path(source_ref).read_bytes()


def _prepare_image(image) -> bytes:
    from PIL import Image

    frame_size = _frame_size()
    rgb = image.convert("RGB")
    width, height = rgb.size
    scale = min(frame_size / max(width, 1), frame_size / max(height, 1))
    scaled_width = max(1, int(round(width * scale)))
    scaled_height = max(1, int(round(height * scale)))
    resized = rgb.resize((scaled_width, scaled_height))
    canvas = Image.new("RGB", (frame_size, frame_size), color=(0, 0, 0))
    offset_x = (frame_size - scaled_width) // 2
    offset_y = (frame_size - scaled_height) // 2
    canvas.paste(resized, (offset_x, offset_y))
    return canvas.tobytes()


def _resolve_model_id(model_alias: str) -> str:
    normalized = model_alias.strip().lower()
    if "/" in model_alias:
        return model_alias
    aliases = {
        "siglip2_base": "google/siglip2-base-patch16-224",
    }
    if normalized not in aliases:
        raise RuntimeError(f"unsupported find model alias in modal worker: {model_alias}")
    return aliases[normalized]


def _frame_step_ms(sample_fps: float) -> int:
    return max(1, int(round(1000.0 / max(sample_fps, 0.001))))


def _query_cache_ttl_secs() -> float:
    return max(1.0, float(os.getenv("MX8_FIND_QUERY_CACHE_TTL_SECS", "1800")))


def _query_cache_max_entries() -> int:
    return max(1, int(os.getenv("MX8_FIND_QUERY_CACHE_MAX_ENTRIES", "10000")))


def _batch_size() -> int:
    return max(1, int(os.getenv("MX8_FIND_BATCH_SIZE", "16")))


def _frame_size() -> int:
    return max(64, int(os.getenv("MX8_FIND_FRAME_SIZE", "224")))


def _score_threshold() -> float:
    return float(os.getenv("MX8_FIND_SCORE_THRESHOLD", "0.25"))


def _pad_before_ms() -> int:
    return max(0, int(os.getenv("MX8_FIND_PAD_BEFORE_MS", "500")))


def _pad_after_ms() -> int:
    return max(0, int(os.getenv("MX8_FIND_PAD_AFTER_MS", "1500")))


def _merge_gap_ms() -> int:
    return max(0, int(os.getenv("MX8_FIND_MERGE_GAP_MS", "1500")))


def _split_rawvideo_frames(buffer: bytes, *, width: int, height: int) -> list[memoryview]:
    if not buffer:
        return []
    frame_bytes = width * height * 3
    if len(buffer) % frame_bytes != 0:
        raise RuntimeError(
            f"ffmpeg returned {len(buffer)} bytes, which is not divisible by raw frame size {frame_bytes}"
        )
    view = memoryview(buffer)
    return [view[offset : offset + frame_bytes] for offset in range(0, len(buffer), frame_bytes)]


def _batched(values: Sequence[Any], batch_size: int) -> Iterable[Sequence[Any]]:
    for index in range(0, len(values), batch_size):
        yield values[index : index + batch_size]
