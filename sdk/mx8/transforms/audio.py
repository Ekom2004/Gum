from __future__ import annotations

from ._base import Transform


def resample(*, rate: int = 16000, channels: int = 1) -> Transform:
    return Transform(
        kind="audio.resample",
        params={
            "rate": rate,
            "channels": channels,
        },
    )


def normalize(*, loudness: float = -14.0) -> Transform:
    return Transform(
        kind="audio.normalize",
        params={
            "loudness": loudness,
        },
    )
