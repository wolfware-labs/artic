use crate::Error;
use crate::backend::{backends_for, is_warp};
use crate::translate::translate;
use artic_render::{
  PhysicalSize, PixelBuffer, PixelFormat, RawHandles, RenderError, Scene, SurfaceId, SurfaceView, Target, TargetKind,
};
use std::collections::HashMap;
use std::sync::mpsc;
use vello::wgpu::util::TextureBlitter;
use vello::wgpu::{
  self, Backends, CompositeAlphaMode, CurrentSurfaceTexture, Device, DeviceDescriptor, Extent3d, Instance, InstanceDescriptor,
  PresentMode, Queue, SurfaceConfiguration, SurfaceTargetUnsafe, TexelCopyBufferInfo, TexelCopyBufferLayout,
  TexelCopyTextureInfo, Texture, TextureFormat, TextureUsages, TextureView,
};
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions};

/// Draws artic scenes with `vello` on one GPU device.
///
/// It accepts [`Target::Handles`], keeping a swapchain per surface until the host reports the
/// surface lost, and [`Target::Pixels`], rendering offscreen and copying the pixels back.
pub struct Renderer {
  instance: Instance,
  adapter: wgpu::Adapter,
  device: Device,
  queue: Queue,
  vello: vello::Renderer,
  /// The translated scene, reused across frames to keep its allocations.
  encoded: vello::Scene,
  swapchains: HashMap<SurfaceId, Swapchain>,
}

/// A window's surface, with the texture vello draws into before it's copied onto the surface.
struct Swapchain {
  surface: wgpu::Surface<'static>,
  config: SurfaceConfiguration,
  target: TextureView,
  blitter: TextureBlitter,
}

impl Renderer {
  /// Opens the GPU and prepares vello's pipelines. It blocks until they're ready.
  ///
  /// # Errors
  ///
  /// Returns a [`Error`] when no adapter supports the platform's GPU API, the one found is WARP,
  /// the device can't be opened, or vello's pipelines can't be built.
  pub fn new() -> Result<Renderer, Error> {
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::from_env().unwrap_or_else(|| backends_for(std::env::consts::OS));
    let instance = Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let info = adapter.get_info();
    if is_warp(info.backend, info.device_type) {
      return Err(Error::UnsupportedAdapter(info.name));
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor::default()))?;
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
      vello,
      encoded: vello::Scene::new(),
      swapchains: HashMap::new(),
    })
  }

  /// Draws the translated scene into a fresh texture of `size`, which vello can write.
  fn draw_offscreen(&mut self, size: PhysicalSize, usage: TextureUsages) -> Result<Texture, RenderError> {
    let texture = storage_texture(&self.device, size, usage);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    self.draw(&view, size)?;
    Ok(texture)
  }

  fn draw(&mut self, view: &TextureView, size: PhysicalSize) -> Result<(), RenderError> {
    let params = RenderParams {
      base_color: vello::peniko::Color::TRANSPARENT,
      width: size.width,
      height: size.height,
      antialiasing_method: AaConfig::Area,
    };
    self
      .vello
      .render_to_texture(&self.device, &self.queue, &self.encoded, view, &params)
      .map_err(backend)
  }

  fn render_to_window(&mut self, id: SurfaceId, size: PhysicalSize, handles: &dyn RawHandles) -> Result<(), RenderError> {
    if size.is_empty() {
      return Ok(());
    }
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
    let frame_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    swapchain
      .blitter
      .copy(&self.device, &mut encoder, &swapchain.target, &frame_view);
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
    handles: &dyn RawHandles,
  ) -> Result<Option<wgpu::SurfaceTexture>, RenderError> {
    for attempt in 0..2 {
      let swapchain = &self.swapchains[&id];
      match swapchain.surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => return Ok(Some(frame)),
        CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(None),
        CurrentSurfaceTexture::Outdated if attempt == 0 => swapchain.surface.configure(&self.device, &swapchain.config),
        CurrentSurfaceTexture::Lost if attempt == 0 => {
          let swapchain = self.create_swapchain(handles, size)?;
          self.swapchains.insert(id, swapchain);
        }
        CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => return Err(RenderError::SurfaceLost),
        CurrentSurfaceTexture::Validation => return Err(backend("the surface texture failed validation")),
      }
    }
    Ok(None)
  }

  fn create_swapchain(&self, handles: &dyn RawHandles, size: PhysicalSize) -> Result<Swapchain, RenderError> {
    let target = SurfaceTargetUnsafe::RawHandle {
      raw_display_handle: Some(handles.display_handle().map_err(backend)?.as_raw()),
      raw_window_handle: handles.window_handle().map_err(backend)?.as_raw(),
    };
    // SAFETY: `Target::Handles` promises the handles stay valid until the host reports the surface
    // lost, and `surface_lost` drops the surface then.
    let surface = unsafe { self.instance.create_surface_unsafe(target) }.map_err(backend)?;
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
    let target = storage_view(&self.device, size);
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
    swapchain.target = storage_view(&self.device, size);
  }

  fn render_to_pixels(&mut self, size: PhysicalSize, buffer: PixelBuffer<'_>) -> Result<(), RenderError> {
    let swap_red_and_blue = match buffer.format {
      PixelFormat::Rgba8 => false,
      PixelFormat::Bgra8 => true,
      other => return Err(RenderError::UnsupportedFormat(other)),
    };
    let row_bytes = size.width as usize * 4;
    if buffer.stride < row_bytes || buffer.data.len() < buffer.stride * size.height as usize {
      return Err(backend("the pixel buffer is smaller than the surface"));
    }
    if size.is_empty() {
      return Ok(());
    }
    let texture = self.draw_offscreen(size, TextureUsages::COPY_SRC)?;
    let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
    let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("artic-vello readback"),
      size: (padded * size.height as usize) as u64,
      usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
      mapped_at_creation: false,
    });
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
      TexelCopyTextureInfo {
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
      },
      TexelCopyBufferInfo {
        buffer: &readback,
        layout: TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(padded as u32),
          rows_per_image: None,
        },
      },
      extent(size),
    );
    self.queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
      let _ = sender.send(result);
    });
    self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(backend)?;
    receiver.recv().map_err(backend)?.map_err(backend)?;
    let mapped = slice.get_mapped_range().map_err(backend)?;
    for (row, source) in buffer.data.chunks_mut(buffer.stride).zip(mapped.chunks(padded)) {
      let row = &mut row[..row_bytes];
      row.copy_from_slice(&source[..row_bytes]);
      if swap_red_and_blue {
        for pixel in row.as_chunks_mut::<4>().0 {
          pixel.swap(0, 2);
        }
      }
    }
    Ok(())
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
    formatter
      .debug_struct("Renderer")
      .field("adapter", &self.adapter.get_info().name)
      .field("surfaces", &self.swapchains.len())
      .finish_non_exhaustive()
  }
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
    usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING | usage,
    view_formats: &[],
  })
}

fn storage_view(device: &Device, size: PhysicalSize) -> TextureView {
  storage_texture(device, size, TextureUsages::empty()).create_view(&wgpu::TextureViewDescriptor::default())
}

/// A library failure, as the `RenderError` the renderer seam reports.
fn backend(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> RenderError {
  RenderError::Backend(error.into())
}
