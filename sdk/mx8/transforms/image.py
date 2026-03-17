from __future__ import annotations

from ._base import Transform


def resize(*, width: int, height: int, maintain_aspect: bool = True) -> Transform:
    return Transform(
        kind="image.resize",
        params={
            "width": width,
            "height": height,
            "maintain_aspect": maintain_aspect,
        },
    )


def convert(*, format: str, quality: int = 85) -> Transform:
    return Transform(
        kind="image.convert",
        params={
            "format": format,
            "quality": quality,
        },
    )
