# mx8-find-media

`mx8-find-media` is the first Rust-owned seam for the `find` media hot path.

Current scope:

- define the extractor request/response contract for sampled RGB frames
- link directly against FFmpeg libraries (`libavformat`, `libavcodec`, `libavutil`, `libswscale`)
- prove the native boundary without going through the `ffmpeg` CLI subprocess

Deliberately out of scope for the first slice:

- full decode implementation
- model scoring
- planner integration
- generic media framework design

Next benchmark target:

- implement one direct-libav extractor for `source_uri + window + fps + frame_size`
- compare it against the current Python worker media stage on the same shard
