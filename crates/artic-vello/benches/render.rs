// What a frame drawn into CPU pixels costs at 4K and 5K: the box shadow golden scene scaled to the
// frame's width, drawn by vello and read back. A 5K frame takes more than one vello pass, so this
// is the bench that shows what the band split costs.
//
// It needs a GPU: without a usable adapter it skips, as the goldens do, and registers nothing. CI's
// benchmark job has no GPU, so compare on a machine with one:
//
//   cargo bench -p artic-vello --bench render -- --save-baseline main
//   cargo bench -p artic-vello --bench render -- --baseline main
//
// Each size checks once, before it's timed, that the frame's corner is the scene's white.

use artic_core::error::Chain;
use artic_render::{PhysicalSize, PixelBuffer, PixelFormat, Renderer as _, Scene, SurfaceId, SurfaceView, Target};
use artic_test::{require, scenes};
use artic_vello::{Error, Renderer};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

const SIZES: [PhysicalSize; 2] = [PhysicalSize::new(3840, 2160), PhysicalSize::new(5120, 2880)];

/// The renderer, or `None` after skipping through [`require::no_gpu`] when there's no usable
/// adapter.
fn renderer() -> Option<Renderer> {
  match Renderer::new() {
    Ok(renderer) => Some(renderer),
    Err(error @ (Error::Adapter(_) | Error::UnsupportedAdapter(_))) => {
      require::no_gpu(Chain(&error));
      None
    }
    Err(error) => panic!("the renderer failed to start: {}", Chain(&error)),
  }
}

/// Draws `scene` into `data`, a frame of `size`, scaled so the scene spans its width.
fn draw(renderer: &mut Renderer, scene: &Scene, size: PhysicalSize, data: &mut [u8]) {
  let scale = size.width as f32 / scenes::SIZE.width as f32;
  let target = Target::Pixels(PixelBuffer {
    data,
    stride: size.width as usize * 4,
    format: PixelFormat::Rgba8,
  });
  let view = SurfaceView {
    id: SurfaceId::new(1),
    size,
    scale,
    target,
  };
  renderer.render(scene, view).expect("renders");
}

fn render_to_pixels(c: &mut Criterion) {
  let Some(mut renderer) = renderer() else {
    return;
  };
  let scene = scenes::box_shadow();
  let mut group = c.benchmark_group("render_to_pixels");
  group.sample_size(20);
  for size in SIZES {
    let mut data = vec![0; size.width as usize * size.height as usize * 4];
    draw(&mut renderer, &scene, size, &mut data);
    assert_eq!(data[..4], [255; 4], "the top left pixel is the scene's white");
    group.bench_function(BenchmarkId::from_parameter(format!("{}x{}", size.width, size.height)), |b| {
      b.iter(|| draw(&mut renderer, &scene, black_box(size), black_box(&mut data)));
    });
  }
  group.finish();
}

criterion_group!(benches, render_to_pixels);
criterion_main!(benches);
