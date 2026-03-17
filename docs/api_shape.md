# MX8 Media API Shape

This doc locks the user-facing API shape for `mx8-media` v0.

We are only locking the call shape here. Transform variables and per-transform options come later.

## Locked Shape

```python
mx8.run(
    "s3://bucket/input/",
    transform=image.resize(...),
    sink="s3://bucket/output/",
)
```

## Rules

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

## Not Locked Yet

- Per-transform variables and defaults
- Shared top-level options beyond `transform` and `sink`
- Exact transform catalog
- Output naming and overwrite behavior
