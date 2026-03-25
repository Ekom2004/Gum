# MX8 Media SDK

MX8 runs video, image, and audio transforms across large media datasets without forcing teams to build their own distributed orchestration layer.

## Install

```bash
pip install mx8
```

## Quick Start

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

## API Shape

- `import mx8` is the primary entry point.
- `mx8.run(input=..., work=[...], output=...)` is the canonical job shape.
- Common operations such as `mx8.find(...)` and `mx8.extract_frames(...)` live at the top level.
- Media namespaces remain available for deeper transform composition, but the default UX is an ordered `work=[...]` list.

See `docs/api_shape.md` in the repository for the current SDK and REST surface.
