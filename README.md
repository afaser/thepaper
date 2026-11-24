# thepaper

Zero-copy live video wallpaper for [niri](https://github.com/YaLTeR/niri).

One VA-API decoder produces DMA-BUF frames; Vulkan imports them directly and
samples the same textures for every output. The overview surface shows a cached
two-pass Gaussian blur of the same frame. Nothing is copied to system memory
per frame.

## Features

- One hardware decoder for all outputs, one shared source frame per frame
  number.
- VA-API decode to `DRM_PRIME` DMA-BUF, imported into Vulkan without readback.
- Live background surface plus a separate overview surface on every output.
- Blur is computed only while the niri overview is open and is cached per frame.
- Game Mode pauses packet reads and moves the decoder worker to `SCHED_IDLE`.
- Constant low priority: `nice 19` and idle I/O class for the whole process.
- One-time automatic transcode when the source codec has no VA-API decode
  profile; no software decoding at runtime.
- Single-instance lock, hotplug and scale handling.
- Audio and all non-video streams are ignored.

## Requirements

| Component | Requirement |
| --- | --- |
| Compositor | niri 26.04+ with `OverviewOpenedOrClosed` in the IPC event stream |
| Session | Wayland; `zwlr_layer_shell_v1`, `wl_output`, `wl_compositor` |
| GPU | Vulkan adapter exposing `VK_EXT_external_memory_dma_buf`, `VK_EXT_image_drm_format_modifier` and the immediates feature |
| Video decode | VA-API hardware decode on a DRM render node (8-bit NV12 output; 10-bit/P010 input is not supported) |
| Tools | `ffmpeg`, `ffprobe`, `vainfo` on `PATH` (provided by the flake) |
| Game Mode | `gamemoded` on the session bus (optional; absence means "no games") |
| Build | Rust 1.89+; Nix flake pins the complete toolchain |

The Vulkan and VA-API devices must be able to share DMA-BUFs. On a hybrid
system, run the compositor and `thepaper` on the same GPU.

## Build

All project commands run through direnv:

```sh
direnv allow
direnv exec . cargo build --release
```

The binary is `target/release/thepaper`. The flake dev shell contains
`rustc`, `cargo`, `rustfmt`, `clippy`, FFmpeg with VA-API, `libva`,
`libva-utils`, the Vulkan loader and headers, Mesa, Wayland, `libdrm`,
`pkg-config`, `clang` and `libclang`. Dependencies come from crates.io only.

## Quick start

```sh
mkdir -p ~/.config/thepaper
$EDITOR ~/.config/thepaper/config.kdl
thepaper
```

Minimal configuration:

```kdl
video_path "/home/user/Videos/wallpaper.mp4"
```

Add the niri rule from [niri setup](#niri-setup) and start `thepaper` with the
compositor.

## Configuration

Default path: `$XDG_CONFIG_HOME/thepaper/config.kdl`
(`~/.config/thepaper/config.kdl`). `--config <path>` overrides it.

Resolution order:

1. `--config <path>`
2. `$XDG_CONFIG_HOME/thepaper/config.kdl` if `XDG_CONFIG_HOME` is absolute
3. `$HOME/.config/thepaper/config.kdl`
4. `.config/thepaper/config.kdl` when neither variable is usable

Relative `XDG_CONFIG_HOME` values are ignored. The file is KDL v2: one
top-level node per option, exactly one argument, no properties and no child
blocks. Comments start with `//`.

```kdl
video_path "/home/user/Videos/wallpaper.mp4"
blur_radius 12
app_id_prefix "thepaper"
// transcode_codec "hevc"
```

### Options

| Option | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `video_path` | string | yes | — | Existing regular video file. One video stream is played and looped; audio is dropped. |
| `blur_radius` | integer | no | `12` | Gaussian blur radius for the overview surface, `1..=64`. |
| `app_id_prefix` | string | no | `"thepaper"` | Non-empty prefix for layer-shell namespaces. |
| `transcode_codec` | string | no | auto | Forces the transcode target: `av1`, `hevc`/`h265`, `vp9` or `h264`/`avc`, case-insensitive. Only used when a transcode is needed. The target must be hardware-decodable and encodable. |

Validation rejects unknown options, duplicate options, missing or extra
arguments, properties, child blocks, wrong argument types, an empty
`video_path`, an empty `app_id_prefix`, a non-existent video and a
`blur_radius` outside `1..=64`. Errors name the configuration path and the
offending option; the process exits before touching Wayland.

### Command line

| Flag | Meaning |
| --- | --- |
| `-c`, `--config <path>` | Configuration file path. |
| `-h`, `--help` | Print usage. |
| `-V`, `--version` | Print version. |

### Paths

| Purpose | Path |
| --- | --- |
| Configuration | `$XDG_CONFIG_HOME/thepaper/config.kdl`, fallback `~/.config/thepaper/config.kdl` |
| Transcode cache | `$XDG_CACHE_HOME/thepaper`, fallback `~/.cache/thepaper`, then the system temp directory |
| Instance lock | `$XDG_RUNTIME_DIR/thepaper.lock`; if `XDG_RUNTIME_DIR` is unset or relative, the cache directory is used |

The transcode cache is not rotated automatically. Remove
`~/.cache/thepaper/*.mkv` to reclaim space; files are recreated on demand.

### Environment variables

| Variable | Default | Effect |
| --- | --- | --- |
| `THEPAPER_DRM_RENDER_NODE` | first existing `/dev/dri/renderD128`…`renderD135`, else `renderD128` | DRM render node used by VA-API and the transcode probe. |
| `THEPAPER_LOG` | `info` | `tracing` env-filter directive. |
| `NIRI_SOCKET` | from the niri session | niri IPC socket. If unset or unreachable, live backgrounds still play and overview tracking is disabled; the client retries every 2 seconds. |
| `XDG_CONFIG_HOME` | `$HOME/.config` | Base for the default configuration path. |
| `XDG_CACHE_HOME` | `$HOME/.cache` | Base for transcodes and the fallback lock. |
| `XDG_RUNTIME_DIR` | cache directory | Base for the instance lock. |
| `HOME` | none | Fallback base for config/cache paths. |
| `WAYLAND_DISPLAY` | from the session | Wayland socket used by the compositor connection. |

## niri setup

```kdl
layer-rule {
    // Put the overview surface into the overview backdrop.
    match namespace="^thepaper-overview-"
    place-within-backdrop true
}

spawn-at-startup "thepaper"
```

Namespace forms:

- background: `<prefix>-<output>`, for example `thepaper-DP-1`
- overview: `<prefix>-overview-<output>`, for example
  `thepaper-overview-DP-1`

Both surfaces use the `background` layer and reserve no space. The background
surface has `exclusive_zone = 0`; the overview surface uses `-1`
(`DontCare`), because niri only accepts that value into the overview backdrop.
The overview surface is mapped only while the overview is open.

Verify the surfaces with:

```sh
niri msg layers
```

## Transcoding

`prepare` first probes the source with FFmpeg and VA-API:

1. `ffprobe` reads the first video stream codec.
2. FFmpeg decodes one frame through `-hwaccel vaapi` on the render node.
3. If that succeeds, the source is played as-is, even when
   `transcode_codec` is set.
4. Otherwise, `vainfo` enumerates VA-API decode/encode profiles, FFmpeg
   enumerates available encoders, and one target is chosen.
5. The transcode is written to the cache, decoded once with VA-API to verify
   it, and then reused on later starts.

Target selection prefers compression efficiency while requiring both a
VA-API decode profile and an encoder:

| Codec | Preferred VA-API encoder | CPU fallback | CPU settings |
| --- | --- | --- | --- |
| AV1 | `av1_vaapi` | `libsvtav1` | `-preset 6 -crf 30` |
| HEVC | `hevc_vaapi` | `libx265` | `-preset medium -crf 23` |
| VP9 | `vp9_vaapi` | `libvpx-vp9` | `-crf 30 -b:v 0` |
| H.264 | `h264_vaapi` | `libx264` | `-preset medium -crf 20` |

VA-API encoders upload as NV12 and use `-qp 23`. CPU encoders write 8-bit
`yuv420p`. Output is Matroska, video-only, metadata stripped.

The cache key covers source path, size, mtime, target codec, encoder and a
format version. The file name is the first 16 bytes of SHA-256 in hex plus
`-<codec>.mkv`. A cached file is reused only if a VA-API frame decodes
successfully.

If the source cannot be hardware-decoded and no codec is both
hardware-decodable and encodable, the process exits with an error. Software
decoding is never used.

## Runtime behaviour

- One `AVFormatContext`, one VA-API `AVCodecContext`, one decoder thread. The
  decoder loops by seeking back to the start at EOF.
- Frames go to a latest-frame mailbox; stale frames are dropped instead of
  queued.
- Each DMA-BUF plane is imported as an `R8`/`Rg8` Vulkan texture. The shader
  performs YUV-to-RGB conversion using the frame's color range, matrix and
  transfer metadata, then cover-scales to the surface.
- A DMA-BUF texture cache is keyed by buffer inode and bounded to 32 entries.
  Up to three submitted frames are tracked until GPU work completes.
- Present mode is `Fifo`; `desired_maximum_frame_latency` is 2. Surface
  formats prefer `Bgra8Unorm`/`Rgba8Unorm`.
- The overview blur is a separable two-pass Gaussian. Sample spacing is
  `max(blur_radius / 6, 0.5)`. The result is cached per frame sequence; no blur
  passes run while the overview is closed.
- Outputs can be added, removed and rescaled at runtime; both surfaces of an
  output are recreated and reconfigured as needed.
- A second instance exits with a clear message; the lock is `flock`-based on
  `thepaper.lock`.

### Game Mode

`thepaper` connects to `com.feralinteractive.GameMode` on the session bus,
subscribes to `GameRegistered`/`GameUnregistered`, and also polls
`ClientCount` once per second. If the service is absent, the session is treated
as having no games; the connection is retried every 5 seconds.

When a game is registered:

- packet reads stop at the control state, independently of the event-loop
  queue;
- the decoder thread moves to `SCHED_IDLE` (falling back to `SCHED_BATCH`);
- retired GPU frames are drained and `malloc_trim(0)` returns unused heap;
- the last frame stays visible.

On resume, the existing decoder state (position and VA-API context) is handed
to a fresh `SCHED_OTHER` worker. Linux does not allow an unprivileged process
to leave `SCHED_IDLE`, so the worker is replaced, not the decoder. `nice 19`
and idle I/O priority remain for the process lifetime; the event-loop thread
stays `SCHED_OTHER`.

### niri IPC

The client subscribes to niri's event stream and reads the initial
`OverviewState`. `OverviewOpenedOrClosed` maps or unmaps every overview surface
and drives blur rendering. A missing or broken IPC channel is non-fatal: only
the live backgrounds play, and the client retries every 2 seconds.

## Performance and benchmarks

The design was driven by a measured zero-copy comparison at 2K @ 15 fps on an
AMD Cezanne / VCN 3.0 system (Mesa `radeonsi`, VA-API 1.23, `renderD128`):

| Pipeline | CPU, one core | Note |
| --- | --- | --- |
| VA-API decode → system RAM → software render → `Queue::write_texture` | ~60% | measured predecessor of this project |
| `mpv --vo=gpu-next --hwdec=vaapi` on the same file | ~2% | reference GPU path |
| `thepaper` | demultiplexing and control only; `mpv`-class target | zero-copy DMA-BUF path, no per-frame readback |

These are reference points from the design phase, not a guarantee for other
hardware or codecs. The repository has no benchmark harness; measure the local
build instead.

Suggested measurement:

```sh
pid=$(pgrep -n thepaper)
ps -o pid,pcpu,rss,nlwp,cmd -p "$pid"
ps -L -o tid,pcpu,comm -p "$pid"
grep -E 'VmRSS|VmHWM|RssAnon' "/proc/$pid/status"
```

For GPU utilization use the vendor tool for the active GPU, for example
`radeontop` or `amdgpu_top` on AMD. Compare four states:

1. overview closed, normal desktop;
2. overview open (blur passes active);
3. game registered (decoder paused, `SCHED_IDLE`);
4. after the game exits (fresh `SCHED_OTHER` worker).

`THEPAPER_LOG=thepaper=debug` logs `decoded frame imported`, `frame ring state`
and `blur_passes_total` every 30 frames. RSS should stay bounded across loops;
the latest-frame mailbox, the 32-entry texture cache and the three-frame
submitted ring are the intended limits.

## Logs and troubleshooting

Default logging is `info`:

```sh
THEPAPER_LOG='thepaper=debug,wgpu=warn,wgpu_core=warn,wgpu_hal=warn,naga=warn' thepaper
```

Useful checks:

```sh
vainfo --display drm --device /dev/dri/renderD128
ffmpeg -hide_banner -hwaccels
vulkaninfo --summary
niri msg layers
niri msg outputs
```

Common failures:

| Message | Cause |
| --- | --- |
| `cannot connect to the Wayland compositor` | Not inside a niri/Wayland session, or `WAYLAND_DISPLAY` is wrong. |
| `expected a Vulkan adapter` / `lacks required features` | Vulkan adapter is missing, is not Vulkan, or lacks DMA-BUF/immediates support. |
| `cannot create VA-API device ... (hardware decode is mandatory)` | Wrong render node or missing VA-API driver. |
| `unsupported DRM_PRIME layer format ... only 8-bit NV12 is supported` | 10-bit/P010 or another unsupported decode output. |
| `no codec is both hardware-decodable and encodable` | Source is not hardware-decodable and no transcode target exists. |
| `another thepaper instance is already running` | The instance lock is held. |
| `missing option \`video_path\`` / `unknown option` | Configuration file does not match the KDL schema. |

## Migration from TOML

`config.toml` is no longer read. Rename the file, remove the `=`, and replace
`#` comments with `//`; values stay the same.

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
`layer-rule` namespace match and in `spawn-at-startup`. The old surface
namespaces (`-live-`/`-blur-`) are not supported.

## Development

The flake is pinned by `flake.lock`; `.envrc` contains `use flake`.

```sh
direnv exec . cargo fmt --check
direnv exec . cargo clippy --all-targets
direnv exec . cargo test
```

Heavy commands (build, check, test, clippy, container builds) run with one
heavy task at a time, low priority and a 10-thread cap:

```sh
chrt -i 0 nice -n 19 ionice -c 3 direnv exec . cargo build --release
```

The application is exercised in a rootless podman container with a nested
headless compositor and a rootless Xwayland client; `cargo run` on the host is
not part of the workflow. A visual run requires a hardware GPU. OpenSpec
specifications live under `openspec/specs/thepaper/`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) for the full text.
