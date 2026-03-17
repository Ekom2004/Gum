from __future__ import annotations

from ._base import Transform


def transcode(*, codec: str, crf: int = 23) -> Transform:
    return Transform(
        kind="video.transcode",
        params={
            "codec": codec,
            "crf": crf,
        },
    )


def resize(*, width: int, height: int, maintain_aspect: bool = True) -> Transform:
    return Transform(
        kind="video.resize",
        params={
            "width": width,
            "height": height,
            "maintain_aspect": maintain_aspect,
        },
    )


def extract_frames(*, fps: float, format: str) -> Transform:
    return Transform(
        kind="video.extract_frames",
        params={
            "fps": fps,
            "format": format,
        },
    )


def extract_audio(*, format: str, bitrate: str = "128k") -> Transform:
    return Transform(
        kind="video.extract_audio",
        params={
            "format": format,
            "bitrate": bitrate,
        },
    )
