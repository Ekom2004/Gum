from __future__ import annotations

from collections.abc import Sequence

from . import transforms as transforms
from .client import MX8Client, default_client
from .job import Job
from .transforms import Transform, TransformChain, audio, image, video
from .transforms.video import extract_frames
from .work import FindWork, find

Client = MX8Client


def run(
    *,
    input: str | None = None,
    work: Transform | FindWork | Sequence[Transform | FindWork] | TransformChain | None = None,
    output: str | None = None,
    source: str | None = None,
    sink: str | None = None,
    find: str | None = None,
    transforms: Transform | Sequence[Transform] | TransformChain | None = None,
    transform: Transform | Sequence[Transform] | TransformChain | None = None,
    client: MX8Client | None = None,
) -> Job:
    normalized_input = input if input is not None else source
    normalized_output = output if output is not None else sink
    if normalized_input is None:
        raise ValueError("`input` is required")
    if normalized_output is None:
        raise ValueError("`output` is required")

    legacy_payload = transforms if transforms is not None else transform
    if work is not None and (legacy_payload is not None or find is not None):
        raise ValueError("use either `work=` or legacy `find`/`transform` arguments, not both")
    if work is None:
        if legacy_payload is None:
            raise ValueError("`work` is required")
        work_items: list[Transform | FindWork] = []
        if find is not None:
            work_items.append(globals()["find"](find))
        if isinstance(legacy_payload, Transform):
            work_items.append(legacy_payload)
        elif isinstance(legacy_payload, TransformChain):
            work_items.extend(legacy_payload.to_transforms())
        else:
            work_items.extend(list(legacy_payload))
        work = work_items

    active_client = client or default_client()
    return active_client.submit_job(input=normalized_input, work=work, output=normalized_output)


__all__ = [
    "Client",
    "FindWork",
    "Job",
    "Transform",
    "TransformChain",
    "audio",
    "extract_frames",
    "find",
    "image",
    "run",
    "video",
    "transforms",
]
