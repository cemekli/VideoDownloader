# FFmpeg fixtures (STATE.md F0.8c, ADR-0006)

- `args.json`: VideoVault's fixed FFmpeg/ffprobe flags. `crates/vv-ffmpeg`
  (`args.rs`) and `scripts/ffmpeg-probe.mjs` both follow it; a Rust test fails
  if they drift.
- `probe/<id>.json`: what the restricted ffprobe prints for each file in
  `tests/fixtures/media/`. `scripts/fetch-tools.mjs` runs the pinned build on
  every CI run and fails if its output differs.
- `rejections.json`: HLS/concat/URL inputs and two controls, with the number
  of requests a local HTTP canary received (AC-59).
- `version-output.json`: real `-version` output (pinned build and a distro
  build used as a negative case).

Rewrite with `node scripts/ffmpeg-probe.mjs --ffmpeg <ffmpeg> --ffprobe <ffprobe> --update`
and commit the result together with the reason in ADR-0006.
