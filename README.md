# thepaper

Zero-copy live video wallpaper for niri: one VA-API decoder, DMA-BUF frames
imported into Vulkan, one shared sharp frame on every output plus a cached
gaussian blur shown only in the niri overview.

## Requirements

- niri 26.04+ (uses `OverviewOpenedOrClosed` from niri IPC)
- Vulkan device with `VK_EXT_external_memory_dma_buf` and
  `VK_EXT_image_drm_format_modifier` (RADV/Mesa on the target machine)
- VA-API decode on a DRM render node (`/dev/dri/renderD128` by default,
  override with `THEPAPER_DRM_RENDER_NODE`)
- `ffmpeg`, `ffprobe`, `vainfo` on `PATH` (provided by the flake)
- gamemoded on the session bus (optional; absence means "no games")

All development/build commands run through direnv:

```sh
direnv allow
direnv exec . cargo build --release
```

## Configuration

Default path: `$XDG_CONFIG_HOME/thepaper/config.kdl`
(`~/.config/thepaper/config.kdl`). Pass `--config <path>` to override.

```kdl
video_path "/home/user/Videos/wallpaper.mp4"
blur_radius 12                  // optional, 1..=64, default 12
app_id_prefix "thepaper"        // optional
// transcode_codec "hevc"       // optional override; normally auto-selected
```

If the source codec has no VA-API decode profile (for example AV1 on this
machine), the source is transcoded once into the most efficient codec that is
both hardware-decodable and encodable (HEVC + `hevc_vaapi` here), cached under
`$XDG_CACHE_HOME/thepaper/`, and verified with a hardware decode before
playback. Software decoding is never used; if neither hardware decode nor a
suitable transcode is possible the process exits with a clear error.

## Migration from TOML

`config.toml` is no longer read. Rename the file, remove the `=` signs, and
replace `#` comments with `//`; values stay the same.

```sh
mv ~/.config/thepaper/config.toml ~/.config/thepaper/config.kdl
```

```diff
-video_path = "/home/user/Videos/wallpaper.mp4"
-blur_radius = 12
+video_path "/home/user/Videos/wallpaper.mp4"
+blur_radius 12
```

## Migration from video-wallpaper

Existing installs move their data once; nothing is read from the old
locations.

```sh
mv ~/.config/video-wallpaper ~/.config/thepaper
rm -rf ~/.cache/video-wallpaper
```

In the niri config, replace `video-wallpaper` with `thepaper` in the
`layer-rule` namespace match and in `spawn-at-startup`.

## niri setup

```kdl
layer-rule {
    // Put the blur surface into the overview backdrop.
    match namespace="^thepaper-overview-"
    place-within-backdrop true
}

spawn-at-startup "thepaper"
```

Surface namespaces are `<prefix>-<output>` and `<prefix>-overview-<output>`.
Both live on the `background` layer. The live surface reserves no space
(`exclusive_zone = 0`). niri only accepts `exclusive_zone = DontCare (-1)` into
the overview backdrop, so the blur surface uses `-1` (it still reserves zero
space); without that it cannot be shown in the overview.

## Behaviour

- One decoder and one source GPU frame per frame number; every output samples
  the same imported textures. The blur cache is one shared texture pair per
  frame, computed only while the overview is open.
- The blur surface stays mapped in niri's backdrop, which niri does not render
  while the overview is closed; therefore the previous blurred frame is frozen
  and no blur passes run outside the overview.
- gamemode pauses packet reads immediately (shared control state, not
  dependent on the event-loop queue). The decoder worker runs as `SCHED_IDLE`
  while a game is active. Linux does not allow an unprivileged process to
  leave `SCHED_IDLE`, so on resume the idle worker hands the existing decoder
  state (position, VA-API context) to a freshly spawned `SCHED_OTHER` worker;
  the decoder is not recreated. `nice 19` and idle I/O priority stay for the
  process lifetime, and the event-loop thread remains `SCHED_OTHER`.
- A single `flock` on `$XDG_RUNTIME_DIR/thepaper.lock` prevents a
  second instance; the second process exits with a clear message.
- Missing or broken niri IPC is non-fatal: only the live backgrounds play and
  the client retries the IPC socket every two seconds.

## Logs

```sh
THEPAPER_LOG='thepaper=debug,wgpu=warn,wgpu_core=warn,wgpu_hal=warn,naga=warn' thepaper
```
