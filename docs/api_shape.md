# MX8 Media API Shape

This doc is a quick API-shape reference for `mx8-media` v0.

The authoritative source of truth is `docs/ARCHITECTURE.md`.

If this file conflicts with `docs/ARCHITECTURE.md`, `ARCHITECTURE.md` wins.

## Locked Shape

```python
mx8.run(
    "s3://bucket/input/",
    transform=image.resize(...),
    sink="s3://bucket/output/",
)
```

## Reference Rules

- Top-level entrypoint is `mx8.run(...)`.
- The source is the first positional argument.
- `transform=` is the only structured operation argument.
- `sink=` is the output destination.
- Transform arguments stay inside the transform call.

## Pattern

```python
mx8.run(src, transform=..., sink=...)
```

## Examples

```python
mx8.run(
    "s3://bucket/images/",
    transform=image.resize(width=512, height=512),
    sink="s3://bucket/output/",
)

mx8.run(
    "s3://bucket/videos/",
    transform=video.transcode(...),
    sink="s3://bucket/output/",
)

mx8.run(
    "s3://bucket/audio/",
    transform=audio.resample(rate=16000),
    sink="s3://bucket/output/",
)
```

## Notes

- This file is intentionally narrow.
- Broader product, control-plane, and v0 scope decisions live in `docs/ARCHITECTURE.md`.
