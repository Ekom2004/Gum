# MX8 API Surface

MX8 exposes one high-level job shape so teams can describe media work without learning the control-plane details behind it.

## Canonical Python shape

- `import mx8`: the primary entry point for job submission.
- `mx8.run(input=..., work=[...], output=...)`: the canonical SDK surface.
- Common operations such as `mx8.find(...)` and `mx8.extract_frames(...)` live at the top level.

The canonical Python flow looks like:

```python
import mx8

job = mx8.run(
    input="s3://customer/dataset",
    work=[
        mx8.find("a person in a car"),
        mx8.extract_frames(fps=1, format="jpg"),
    ],
    output="s3://clean/",
)

job.wait()
```

`work` is an ordered list of operations:

- `mx8.find("a person in a car")`: narrows the current media set or segment set before downstream work runs.
- `mx8.extract_frames(fps=1, format="jpg")`: reads frames from the selected video and emits image outputs.
- `output="s3://clean/"`: declares where finished outputs land.

Media namespaces such as `mx8.video`, `mx8.image`, and `mx8.audio` remain available for deeper composition, but the public API is centered on `input`, `work`, and `output`.

## REST endpoints

| Verb | Path | Description |
|------|------|-------------|
| `POST` | `/v1/jobs` | Submit a job. The request body uses `input`, `work`, and `output`.
| `GET` | `/v1/jobs/{job_id}` | Fetch status and progress for one job.
| `GET` | `/v1/jobs` | List jobs for the current team or environment.
| `POST` | `/v1/search` | (Optional) Run a lightweight search query over indexed metadata or vectors. You pay per query; refer to `docs/v0_spec.md` for pricing guidance.

### Job payload

```jsonc
{
  "input": "s3://media-bucket/2026/",
  "work": [
    {"type": "find", "params": {"query": "a stop sign covered in heavy snow"}},
    {"type": "video.extract_frames", "params": {"fps": 1, "format": "jpg"}}
  ],
  "output": "s3://clean/output/"
}
```

The API still accepts the older `source/find/transforms/sink` request shape for compatibility, but `input/work/output` is the canonical public surface.

## CLI helpers

```
mx8 job submit --input s3://bucket --work find="a person in a car" --work extract_frames,fps=1,format=jpg --output s3://clean/
mx8 job pause <id>
mx8 job resume <id>
mx8 job status <id>
```

The CLI reuses the same payload shape as the SDK. Scheduler concerns such as worker concurrency and pool selection stay inside the control plane rather than showing up in the public API.
