# Performance tuning

How to measure an artic app's startup, memory and CPU, and what to change when they are higher than they should be. Each section says what artic does by default and what an app or its user can set.

## Measure first

- **Use a release build.** `cargo run --release` or the binary in `target/release`. A debug build is several times slower and larger.
- **Measure a warm run.** The first run on a machine, after a GPU driver update or after the driver's shader cache is cleared, compiles the renderer's shaders (see [Shaders](#shaders)); rebuilding the app doesn't clear that cache. Report both, but tune against the warm run.
- **Read PSS, not only RSS.** RSS counts every page of the process that is in memory, including the pages of shared libraries, such as GPU driver code, that other processes share. PSS splits shared pages between the processes that use them, so it is closer to what the app adds. `RssAnon` is the resident anonymous memory: every heap, stack and buffer in the process, including those the GPU driver allocates. `RssFile` is the resident memory backed by files: code and data of the binary and the libraries, the drivers' among them. On Linux:

  ```sh
  pid=$(pgrep -x my-app)
  grep -E 'VmRSS|RssAnon|RssFile' /proc/$pid/status
  grep Pss /proc/$pid/smaps_rollup
  ```

- **Idle CPU should be zero for a still window.** artic draws a frame only when something changed, so a window whose content doesn't move uses no CPU between the frames the system asks for, such as after a resize. A `wants_frame` hook that asks for frames, for an animation, keeps it drawing. Sample it over a few seconds after startup, for example from `/proc/$pid/stat`.

A reference point, `examples/hello` in release on a laptop with an Intel and an NVIDIA GPU (Linux, KDE Plasma on Wayland):

| Run | Startup | Peak RSS | Steady RSS | PSS |
| --- | --- | --- | --- | --- |
| first run, shader caches cold | about 3.5 s | 272 MB | | |
| warm | 0.25 s | 165 MB | 154 MB | 80 MB |
| warm, only the Intel Vulkan driver loaded | 0.10 s | | 58 MB | 48 MB |

Idle CPU was 0% in every case.

## GPU choice

The `vello` renderer draws on the GPU through wgpu, which on Linux uses Vulkan.

- **Which GPU.** Today `artic_vello::Renderer::new()` sets no preference, so wgpu takes the first adapter the driver lists. On a machine with a discrete GPU that can be the discrete one, which costs about 1.5 s of startup when it has to wake, and battery life. Asking for the low-power GPU by default, and honoring `WGPU_POWER_PREF` and `WGPU_ADAPTER_NAME`, is #390; choosing the GPU by the window it draws to is #391.
- **Which backend.** The renderer uses Vulkan on Linux, Metal on macOS and iOS, and DX12 on Windows. `WGPU_BACKEND` overrides that choice, as in other wgpu apps, but only a backend the platform has and that runs compute shaders can draw.

## GPU drivers

On Linux, the Vulkan loader loads every installed Vulkan driver when the renderer starts, whichever GPU it then uses. On a machine with several drivers (NVIDIA, Intel, Mesa's software renderer `lavapipe` with its LLVM, `radeon`, and others), their code is most of the process's RSS: in the table above, about 85 MB of the 154 MB.

An app can't easily change this, but its user can tell the loader which drivers to load:

```sh
VK_LOADER_DRIVERS_SELECT='*intel_icd*' ./my-app
```

The pattern matches the driver's manifest file name in `/usr/share/vulkan/icd.d/`. A pattern that matches no driver leaves the renderer with no GPU, and the app exits with an error.

## Shaders

The renderer builds its compute pipelines when it starts. The first run compiles them for the GPU, which took about 3.5 s and 110 MB of extra memory in the table above; later runs read them from the driver's cache (Mesa's in `~/.cache/mesa_shader_cache`, NVIDIA's in `~/.cache/nvidia/GLCache`). Clearing that cache, or a driver update, brings the cost back once.

To see the cold cost on purpose, turn the caches off for one run:

```sh
MESA_SHADER_CACHE_DISABLE=true __GL_SHADER_DISK_CACHE=0 ./my-app
```
