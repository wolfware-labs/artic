use crate::Error;
use crate::backend::{ADAPTER_NAME_VARIABLE, adapter_name, backends_for, is_warp, named, options, power_for};
use crate::translate::translate;
use artic_render::{
  PhysicalSize, PixelBuffer, PixelFormat, RawHandles, RenderError, Scene, SurfaceId, SurfaceView, Target, TargetKind,
};
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::Duration;
use vello::kurbo::Affine;
use vello::wgpu::util::TextureBlitter;
use vello::wgpu::{
  self, Adapter, Backends, CompositeAlphaMode, CurrentSurfaceTexture, Device, DeviceDescriptor, Extent3d, Instance,
  InstanceDescriptor, Limits, PresentMode, Queue, SurfaceConfiguration, TexelCopyBufferInfo, TexelCopyBufferLayout,
  TexelCopyTextureInfo, Texture, TextureFormat, TextureUsages, TextureView,
};
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

/// Draws artic scenes with `vello` on one GPU device.
///
/// It accepts [`Target::Handles`], keeping a swapchain per surface until the host reports the
/// surface lost, and [`Target::Pixels`], rendering offscreen and copying the pixels back.
pub struct Renderer {
  /// The wgpu instance windows' surfaces are created from.
  instance: Instance,
  /// The GPU the device was opened on.
  adapter: wgpu::Adapter,
  /// The device every texture and buffer is made on.
  device: Device,
  /// The device's queue, where draws and copies are submitted.
  queue: Queue,
  /// The longest side, in pixels, of a texture the device can make.
  max_side: u32,
  /// vello's pipelines, built once for the device.
  vello: vello::Renderer,
  /// The translated scene, reused across frames to keep its allocations.
  encoded: vello::Scene,
  /// One band of a frame too large for a single pass, shifted to the band's first row.
  band: vello::Scene,
  /// The swapchain of each window surface, until the host reports it lost.
  swapchains: HashMap<SurfaceId, Swapchain>,
}

/// A window's surface, with the texture vello draws into before it's copied onto the surface.
struct Swapchain {
  surface: wgpu::Surface<'static>,
  config: SurfaceConfiguration,
  target: Texture,
  blitter: TextureBlitter,
}

impl Renderer {
  /// Opens the GPU and prepares vello's pipelines. It blocks until they're ready.
  ///
  /// It asks for the low-power GPU, which is plenty for a 2D UI. As in other wgpu apps,
  /// `WGPU_BACKEND` picks the GPU API, `WGPU_POWER_PREF` (`low`, `high` or `none`) the preference,
  /// and `WGPU_ADAPTER_NAME` the first adapter whose name contains it, ignoring case, whatever the
  /// preference. A blank `WGPU_ADAPTER_NAME` counts as unset.
  ///
  /// # Errors
  ///
  /// Returns a [`Error`] when no adapter supports the platform's GPU API, no adapter's name contains
  /// `WGPU_ADAPTER_NAME`, the one found is WARP, the device can't be opened, or vello's pipelines
  /// can't be built.
  pub fn new() -> Result<Renderer, Error> {
    let backends = Backends::from_env().unwrap_or_else(|| backends_for(std::env::consts::OS));
    let instance = instance(backends);
    let wanted = adapter_name(std::env::var(ADAPTER_NAME_VARIABLE).ok());
    let power = power_for(wgpu::PowerPreference::from_env());
    let adapter = pollster::block_on(adapter(&instance, backends, wanted, power))?;
    let info = adapter.get_info();
    if is_warp(info.backend, info.device_type) {
      return Err(Error::UnsupportedAdapter(info.name));
    }
    let descriptor = DeviceDescriptor {
      required_limits: device_limits(adapter.limits()),
      ..DeviceDescriptor::default()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))?;
    let max_side = device.limits().max_texture_dimension_2d;
    let options = RendererOptions {
      antialiasing_support: AaSupport::area_only(),
      ..RendererOptions::default()
    };
    let vello = vello::Renderer::new(&device, options)?;
    Ok(Renderer {
      instance,
      adapter,
      device,
      queue,
      max_side,
      vello,
      encoded: vello::Scene::new(),
      band: vello::Scene::new(),
      swapchains: HashMap::new(),
    })
  }

  /// Draws the translated scene into a fresh texture of `size`, which vello can write.
  fn draw_offscreen(&mut self, size: PhysicalSize, usage: TextureUsages) -> Result<Texture, RenderError> {
    let texture = storage_texture(&self.device, size, usage);
    self.draw(&texture, size)?;
    Ok(texture)
  }

  /// Draws the translated scene into `target`, of `size`. A frame with more tiles than one vello
  /// pass holds is drawn in bands of whole rows, each shifted up into a texture of its own and
  /// copied into place.
  fn draw(&mut self, target: &Texture, size: PhysicalSize) -> Result<(), RenderError> {
    let rows = rows_per_pass(size.width);
    if rows >= size.height {
      let view = target.create_view(&wgpu::TextureViewDescriptor::default());
      return pass(&mut self.vello, &self.device, &self.queue, &self.encoded, &view, size);
    }
    let band = storage_texture(&self.device, PhysicalSize::new(size.width, rows), TextureUsages::COPY_SRC);
    let view = band.create_view(&wgpu::TextureViewDescriptor::default());
    for (first, part) in bands(size, rows) {
      self.band.reset();
      self
        .band
        .append(&self.encoded, Some(Affine::translate((0.0, -f64::from(first)))));
      pass(&mut self.vello, &self.device, &self.queue, &self.band, &view, part)?;
      let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
      encoder.copy_texture_to_texture(
        band.as_image_copy(),
        TexelCopyTextureInfo {
          origin: wgpu::Origin3d { x: 0, y: first, z: 0 },
          ..target.as_image_copy()
        },
        extent(part),
      );
      self.queue.submit([encoder.finish()]);
    }
    Ok(())
  }

  fn render_to_window(&mut self, id: SurfaceId, size: PhysicalSize, handles: &Arc<dyn RawHandles>) -> Result<(), RenderError> {
    if size.is_empty() {
      return Ok(());
    }
    fits(size, self.max_side)?;
    if !self.swapchains.contains_key(&id) {
      let swapchain = self.create_swapchain(handles, size)?;
      self.swapchains.insert(id, swapchain);
    }
    self.resize(id, size);
    let Some(frame) = self.acquire(id, size, handles)? else {
      return Ok(());
    };
    let swapchain = &self.swapchains[&id];
    let target = swapchain.target.clone();
    self.draw(&target, size)?;
    let swapchain = &self.swapchains[&id];
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let frame_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    swapchain.blitter.copy(&self.device, &mut encoder, &target_view, &frame_view);
    self.queue.submit([encoder.finish()]);
    self.queue.present(frame);
    Ok(())
  }

  /// The next texture of `id`'s surface, reconfiguring once when it's outdated and recreating it
  /// once when it's lost. `None` skips the frame: the window is hidden or the GPU timed out.
  fn acquire(
    &mut self,
    id: SurfaceId,
    size: PhysicalSize,
    handles: &Arc<dyn RawHandles>,
  ) -> Result<Option<wgpu::SurfaceTexture>, RenderError> {
    for retry in [false, true] {
      let swapchain = &self.swapchains[&id];
      match acquired(swapchain.surface.get_current_texture(), retry) {
        Acquired::Frame(frame) => return Ok(Some(frame)),
        Acquired::Skip => return Ok(None),
        Acquired::Reconfigure => swapchain.surface.configure(&self.device, &swapchain.config),
        Acquired::Recreate => {
          let swapchain = self.create_swapchain(handles, size)?;
          self.swapchains.insert(id, swapchain);
        }
        Acquired::Failed(error) => return Err(error),
      }
    }
    Ok(None)
  }

  fn create_swapchain(&self, handles: &Arc<dyn RawHandles>, size: PhysicalSize) -> Result<Swapchain, RenderError> {
    let surface = self.instance.create_surface(Arc::clone(handles)).map_err(backend)?;
    let adapter_formats = surface.get_capabilities(&self.adapter).formats;
    let format = adapter_formats
      .into_iter()
      .find(|format| matches!(format, TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm))
      .ok_or_else(|| backend("the surface supports neither RGBA8 nor BGRA8"))?;
    let config = SurfaceConfiguration {
      usage: TextureUsages::RENDER_ATTACHMENT,
      format,
      width: size.width,
      height: size.height,
      present_mode: PresentMode::Fifo,
      desired_maximum_frame_latency: 2,
      alpha_mode: CompositeAlphaMode::Auto,
      view_formats: Vec::new(),
      color_space: wgpu::SurfaceColorSpace::Auto,
    };
    surface.configure(&self.device, &config);
    let target = storage_texture(&self.device, size, TextureUsages::empty());
    Ok(Swapchain {
      surface,
      config,
      target,
      blitter: TextureBlitter::new(&self.device, format),
    })
  }

  /// Reconfigures `id`'s surface when its size changed.
  fn resize(&mut self, id: SurfaceId, size: PhysicalSize) {
    let swapchain = self.swapchains.get_mut(&id).expect("created before resizing");
    if (swapchain.config.width, swapchain.config.height) == (size.width, size.height) {
      return;
    }
    swapchain.config.width = size.width;
    swapchain.config.height = size.height;
    swapchain.surface.configure(&self.device, &swapchain.config);
    swapchain.target = storage_texture(&self.device, size, TextureUsages::empty());
  }

  fn render_to_pixels(&mut self, size: PhysicalSize, buffer: PixelBuffer<'_>) -> Result<(), RenderError> {
    let swap_red_and_blue = match buffer.format {
      PixelFormat::Rgba8 => false,
      PixelFormat::Bgra8 => true,
      other => return Err(RenderError::UnsupportedFormat(other)),
    };
    if size.is_empty() {
      return Ok(());
    }
    fits(size, self.max_side)?;
    let row_bytes = row_bytes(size, buffer.stride, buffer.data.len())?;
    let texture = self.draw_offscreen(size, TextureUsages::COPY_SRC)?;
    let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
    let rows = rows_per_copy(padded, self.device.limits().max_buffer_size, size.height);
    let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("artic-vello readback"),
      size: padded as u64 * u64::from(rows),
      usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
      mapped_at_creation: false,
    });
    for (first, band) in bands(size, rows) {
      let slice = self.read_back(&texture, first, band, &readback, padded)?;
      let mapped = slice.get_mapped_range().map_err(backend)?;
      let start = first as usize * buffer.stride;
      for (row, source) in buffer.data[start..].chunks_mut(buffer.stride).zip(mapped.chunks(padded)) {
        let row = &mut row[..row_bytes];
        row.copy_from_slice(&source[..row_bytes]);
        if swap_red_and_blue {
          for pixel in row.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
          }
        }
      }
      drop(mapped);
      readback.unmap();
    }
    Ok(())
  }

  /// Copies `band`, the texture's rows from `first` on, into `readback` with rows `padded` bytes
  /// apart, and returns the part of `readback` that holds them, mapped for reading.
  fn read_back<'buffer>(
    &self,
    texture: &Texture,
    first: u32,
    band: PhysicalSize,
    readback: &'buffer wgpu::Buffer,
    padded: usize,
  ) -> Result<wgpu::BufferSlice<'buffer>, RenderError> {
    let bytes_per_row = u32::try_from(padded).map_err(backend)?;
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
      TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d { x: 0, y: first, z: 0 },
        aspect: wgpu::TextureAspect::All,
      },
      TexelCopyBufferInfo {
        buffer: readback,
        layout: TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(bytes_per_row),
          rows_per_image: None,
        },
      },
      extent(band),
    );
    self.queue.submit([encoder.finish()]);
    let slice = readback.slice(..padded as u64 * u64::from(band.height));
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
      let _ = sender.send(result);
    });
    let wait = wgpu::PollType::Wait {
      submission_index: None,
      timeout: Some(READBACK_TIMEOUT),
    };
    self.device.poll(wait).map_err(backend)?;
    receiver.recv_timeout(READBACK_TIMEOUT).map_err(backend)?.map_err(backend)?;
    Ok(slice)
  }
}

impl artic_render::Renderer for Renderer {
  fn accepts(&self, kind: TargetKind) -> bool {
    matches!(kind, TargetKind::Handles | TargetKind::Pixels)
  }

  fn render(&mut self, scene: &Scene, surface: SurfaceView<'_>) -> Result<(), RenderError> {
    translate(scene, surface.scale, surface.size, &mut self.encoded);
    match surface.target {
      Target::Handles(handles) => self.render_to_window(surface.id, surface.size, handles),
      Target::Pixels(buffer) => self.render_to_pixels(surface.size, buffer),
      Target::Custom(_) => Err(RenderError::Unsupported(TargetKind::Custom)),
    }
  }

  fn surface_lost(&mut self, surface: SurfaceId) {
    self.swapchains.remove(&surface);
  }
}

impl std::fmt::Debug for Renderer {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let info = self.adapter.get_info();
    formatter
      .debug_struct("Renderer")
      .field("adapter", &info.name)
      .field("backend", &info.backend)
      .field("surfaces", &self.swapchains.len())
      .finish_non_exhaustive()
  }
}

/// What `acquire` does with the surface's answer, on the first try or the retry.
enum Acquired {
  Frame(wgpu::SurfaceTexture),
  Skip,
  Reconfigure,
  Recreate,
  Failed(RenderError),
}

fn acquired(answer: CurrentSurfaceTexture, retry: bool) -> Acquired {
  match answer {
    CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => Acquired::Frame(frame),
    CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => Acquired::Skip,
    CurrentSurfaceTexture::Outdated if !retry => Acquired::Reconfigure,
    CurrentSurfaceTexture::Lost if !retry => Acquired::Recreate,
    CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => Acquired::Failed(RenderError::SurfaceLost),
    CurrentSurfaceTexture::Validation => Acquired::Failed(backend("the surface texture failed validation")),
  }
}

/// [`RenderError::SurfaceTooLarge`] when a side of `size` is longer than `max`.
fn fits(size: PhysicalSize, max: u32) -> Result<(), RenderError> {
  if size.width > max || size.height > max {
    return Err(RenderError::SurfaceTooLarge { size, max });
  }
  Ok(())
}

/// How long a readback waits for the GPU, and then for the mapped buffer, before it fails, so a
/// GPU that never finishes can't hang the frame.
const READBACK_TIMEOUT: Duration = Duration::from_secs(60);

/// The bytes one row of `size` takes in a pixel buffer `len` bytes long with rows `stride` bytes
/// apart, or an error when the buffer can't hold every row. The products are checked, so they
/// can't wrap where `usize` is 32 bits.
fn row_bytes(size: PhysicalSize, stride: usize, len: usize) -> Result<usize, RenderError> {
  let row = usize::try_from(size.width).ok().and_then(|width| width.checked_mul(4));
  let needed = (stride as u64).checked_mul(u64::from(size.height));
  match (row, needed) {
    (Some(row), Some(needed)) if stride >= row && len as u64 >= needed => Ok(row),
    _ => Err(backend("the pixel buffer is smaller than the surface")),
  }
}

/// How many rows `padded` bytes long one copy back from the GPU holds: all `height` of them when
/// they fit in `max_bytes`, else as many as fit.
fn rows_per_copy(padded: usize, max_bytes: u64, height: u32) -> u32 {
  (max_bytes / padded as u64).min(u64::from(height)) as u32
}

/// The bands of whole rows a frame of `size` splits into, `rows` high but the last: each band's
/// first row and its size.
fn bands(size: PhysicalSize, rows: u32) -> impl Iterator<Item = (u32, PhysicalSize)> {
  (0..size.height)
    .step_by(rows as usize)
    .map(move |first| (first, PhysicalSize::new(size.width, rows.min(size.height - first))))
}

/// The tiles one vello pass draws at most. vello keeps a fixed 2^23 words for the tiles' command
/// lists and reserves 64 for each tile up front, so a pass of 2^17 tiles leaves nothing for a tile
/// that needs more; half of that leaves the other half to them.
const PASS_TILES: u32 = 1 << 16;

/// The side of vello's square tiles, in pixels.
const TILE: u32 = 16;

/// How many rows of a frame `width` pixels wide one vello pass draws: whole rows of tiles, as many
/// as [`PASS_TILES`] holds, and at least one.
fn rows_per_pass(width: u32) -> u32 {
  (PASS_TILES / width.div_ceil(TILE)).max(1) * TILE
}

/// Draws `scene` into `view` at `size` in one vello pass.
fn pass(
  vello: &mut vello::Renderer,
  device: &Device,
  queue: &Queue,
  scene: &vello::Scene,
  view: &TextureView,
  size: PhysicalSize,
) -> Result<(), RenderError> {
  let params = RenderParams {
    base_color: vello::peniko::Color::TRANSPARENT,
    width: size.width,
    height: size.height,
    antialiasing_method: AaConfig::Area,
  };
  vello.render_to_texture(device, queue, scene, view, &params).map_err(backend)
}

fn extent(size: PhysicalSize) -> Extent3d {
  Extent3d {
    width: size.width,
    height: size.height,
    depth_or_array_layers: 1,
  }
}

/// A texture vello can draw into, in the only format its compute shaders write.
fn storage_texture(device: &Device, size: PhysicalSize, usage: TextureUsages) -> Texture {
  device.create_texture(&wgpu::TextureDescriptor {
    label: Some("artic-vello target"),
    size: extent(size),
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: TextureFormat::Rgba8Unorm,
    usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | usage,
    view_formats: &[],
  })
}

/// A library failure, as the `RenderError` the renderer seam reports.
fn backend(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> RenderError {
  RenderError::Backend(error.into())
}

/// A wgpu instance on `backends`.
fn instance(backends: Backends) -> Instance {
  let mut descriptor = InstanceDescriptor::new_without_display_handle();
  descriptor.backends = backends;
  Instance::new(descriptor)
}

/// The limits to open a device with: wgpu's defaults, but the texture sides `adapter` allows.
fn device_limits(adapter: Limits) -> Limits {
  Limits::default().using_resolution(adapter)
}

/// The first adapter on `backends` whose name contains `wanted`, or without `wanted` the one wgpu
/// picks for `power`.
async fn adapter(
  instance: &Instance,
  backends: Backends,
  wanted: Option<String>,
  power: wgpu::PowerPreference,
) -> Result<Adapter, Error> {
  let Some(wanted) = wanted else {
    return Ok(instance.request_adapter(&options(power)).await?);
  };
  let mut adapters = instance.enumerate_adapters(backends).await;
  let names: Vec<String> = adapters.iter().map(|adapter| adapter.get_info().name).collect();
  match named(&wanted, names.iter().map(String::as_str)) {
    Some(index) => Ok(adapters.swap_remove(index)),
    None => Err(Error::NoAdapterNamed {
      wanted,
      available: names,
    }),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_test::require;

  /// An instance on the platform's GPU API, its backends, and the names of its adapters.
  fn platform() -> (Instance, Backends, Vec<String>) {
    let backends = backends_for(std::env::consts::OS);
    let instance = instance(backends);
    let adapters = pollster::block_on(instance.enumerate_adapters(backends));
    let names = adapters.iter().map(|adapter| adapter.get_info().name).collect();
    (instance, backends, names)
  }

  /// Runs `test` with [`platform`]'s instance, backends and adapter names, or skips it through
  /// [`require::no_gpu`] with `reason` when there is no adapter.
  fn with_adapters(reason: &str, test: impl FnOnce(&Instance, Backends, &[String])) {
    let (instance, backends, names) = platform();
    if names.is_empty() {
      require::no_gpu(reason);
    } else {
      test(&instance, backends, &names);
    }
  }

  // A name no adapter has is refused with the adapters there are, rather than falling back to the
  // power preference.
  #[test]
  fn an_unknown_adapter_name_lists_the_adapters() {
    let (instance, backends, names) = platform();
    let wanted = "no adapter has this name".to_owned();

    let chosen = pollster::block_on(adapter(
      &instance,
      backends,
      Some(wanted.clone()),
      wgpu::PowerPreference::LowPower,
    ));

    assert!(matches!(
      chosen,
      Err(Error::NoAdapterNamed { wanted: asked, available }) if asked == wanted && available == names
    ));
  }

  // Each adapter's name, in another case, picks the first adapter with that name, which isn't
  // the one the power preference would pick when the machine has more than one.
  #[test]
  fn an_adapter_name_picks_that_adapter() {
    with_adapters("no adapter to name", |instance, backends, names| {
      for name in names {
        let wanted = name.to_uppercase();
        let chosen = pollster::block_on(adapter(
          instance,
          backends,
          Some(wanted.clone()),
          wgpu::PowerPreference::LowPower,
        ));

        let first = names
          .iter()
          .find(|other| other.to_lowercase().contains(&wanted.to_lowercase()));
        assert_eq!(chosen.ok().map(|adapter| adapter.get_info().name).as_ref(), first, "{name}");
      }
    });
  }

  // An instance turns on only the backends it is given, so one with none lists no adapter even on
  // a machine with some.
  #[test]
  fn an_instance_has_only_its_backends() {
    with_adapters("no adapter to tell backends apart", |_, _, _| {
      let none = instance(Backends::empty());

      assert!(pollster::block_on(none.enumerate_adapters(Backends::all())).is_empty());
    });
  }

  // The device asks for the adapter's own texture limit rather than wgpu's default of 8192, and
  // keeps every other default, such as the buffer size readback splits frames by.
  #[test]
  fn the_device_takes_the_adapters_texture_limit() {
    let adapter = Limits {
      max_texture_dimension_2d: 16384,
      max_buffer_size: u64::MAX,
      ..Limits::default()
    };

    let expected = Limits {
      max_texture_dimension_2d: 16384,
      ..Limits::default()
    };
    assert_eq!(device_limits(adapter), expected);
  }

  // The first outdated answer reconfigures the surface and the first lost one recreates it; on the
  // retry either fails.
  #[test]
  fn an_outdated_or_lost_surface_is_retried_once() {
    assert!(matches!(
      acquired(CurrentSurfaceTexture::Outdated, false),
      Acquired::Reconfigure
    ));
    assert!(matches!(acquired(CurrentSurfaceTexture::Lost, false), Acquired::Recreate));
    let lost = |answer| matches!(acquired(answer, true), Acquired::Failed(RenderError::SurfaceLost));
    assert!(lost(CurrentSurfaceTexture::Outdated), "outdated on the retry");
    assert!(lost(CurrentSurfaceTexture::Lost), "lost on the retry");
  }

  // A hidden window or a timed-out GPU skips the frame; a validation failure is an error.
  #[test]
  fn a_hidden_or_failed_surface_gives_no_frame() {
    for retry in [false, true] {
      assert!(matches!(acquired(CurrentSurfaceTexture::Timeout, retry), Acquired::Skip));
      assert!(matches!(acquired(CurrentSurfaceTexture::Occluded, retry), Acquired::Skip));
      let failed = acquired(CurrentSurfaceTexture::Validation, retry);
      assert!(matches!(failed, Acquired::Failed(RenderError::Backend(_))));
    }
  }

  // A side longer than the limit is refused, whichever side it is; sides at the limit fit.
  #[test]
  fn a_size_fits_up_to_the_limit() {
    let too_large = |width, height| {
      matches!(
        fits(PhysicalSize::new(width, height), 16),
        Err(RenderError::SurfaceTooLarge { size, max: 16 }) if size == PhysicalSize::new(width, height)
      )
    };
    assert!(fits(PhysicalSize::new(16, 16), 16).is_ok());
    assert!(too_large(17, 1), "too wide");
    assert!(too_large(1, 17), "too tall");
  }

  // A pass holds whole rows of tiles up to the budget: a 4K frame is one pass, the widest frames
  // still get a row of tiles, and a width that ends mid-tile counts the whole tile.
  #[test]
  fn a_pass_holds_whole_rows_of_tiles_up_to_the_budget() {
    assert_eq!(rows_per_pass(3840), 4368, "4K, 240 tiles wide");
    assert_eq!(rows_per_pass(16 * 1024), 64 * 16);
    assert_eq!(rows_per_pass(16 * 1024 - 1), 64 * 16, "the last tile partly covered");
    assert_eq!(rows_per_pass(u32::MAX), 16, "wider than the budget");
  }

  // A frame splits into bands of whole rows from the top, the last holding what's left; a band as
  // tall as the frame or taller is the whole frame.
  #[test]
  fn a_frame_splits_into_bands_of_whole_rows() {
    let size = |height| PhysicalSize::new(3, height);
    let split = bands(size(10), 4).collect::<Vec<_>>();
    assert_eq!(split, [(0, size(4)), (4, size(4)), (8, size(2))]);
    assert_eq!(
      bands(size(8), 4).collect::<Vec<_>>(),
      [(0, size(4)), (4, size(4))],
      "even split"
    );
    assert_eq!(
      bands(size(4), 16).collect::<Vec<_>>(),
      [(0, size(4))],
      "taller than the frame"
    );
  }

  // A buffer holds the surface when its rows fit the row and its length fits every row, compared
  // without wrapping: sizes past `u64` and past a 32-bit `usize` are refused, not let through.
  #[test]
  fn a_buffer_holds_the_surface_without_wrapping() {
    let size = PhysicalSize::new(4, 3);
    assert_eq!(row_bytes(size, 16, 48).ok(), Some(16), "exactly the surface");
    assert_eq!(row_bytes(size, 20, 60).ok(), Some(16), "padded rows");
    assert!(row_bytes(size, 15, 48).is_err(), "rows too short");
    assert!(row_bytes(size, 16, 47).is_err(), "one byte short");
    let tall = PhysicalSize::new(4, u32::MAX);
    assert!(row_bytes(tall, 1 << 31, 1 << 20).is_err(), "past a 32-bit usize");
    assert!(row_bytes(tall, usize::MAX, usize::MAX).is_err(), "past u64");
    let wide = PhysicalSize::new(u32::MAX, 1);
    #[cfg(target_pointer_width = "32")]
    assert!(row_bytes(wide, usize::MAX, usize::MAX).is_err(), "a row past a 32-bit usize");
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
      row_bytes(wide, usize::MAX, usize::MAX).ok(),
      Some(u32::MAX as usize * 4),
      "a row past u32"
    );
  }

  // Every row is copied back at once when the rows fit in the limit, and as many whole rows as fit
  // when they don't.
  #[test]
  fn rows_are_copied_back_as_many_as_fit() {
    assert_eq!(rows_per_copy(256, 256 * 4, 4), 4, "exactly the limit");
    assert_eq!(rows_per_copy(256, 256 * 8, 4), 4, "under the limit");
    assert_eq!(rows_per_copy(256, 256 * 4 - 1, 4), 3, "one byte short");
    assert_eq!(rows_per_copy(256, 256 * 2, 4), 2, "half of them");
  }
}
