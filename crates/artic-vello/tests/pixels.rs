// Draws composed scenes into pixel buffers through the `Renderer` trait and checks the pixels.
// Tests skip when the machine has no GPU adapter, or only WARP; see `common::with_renderer`.

mod common;

use artic_core::Object;
use artic_core::property::AnyProperty;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_layout::geometry::{CornerRadius, FillRule, Path, Point, Rect, RelativePoint, RoundedRect, Transform};
use artic_layout::text::{FontFamily, FontStyle, FontWeight, Paragraph, ParagraphStyle, TextEngine, TextLimits, TextStyle};
use artic_render::Renderer as _;
use artic_render::{
  BoxShadow, Color, Compositor, GradientStop, Layers, LinearGradientBrush, Nodes, PhysicalSize, PixelBuffer, PixelFormat,
  RadialGradientBrush, RenderError, SceneBuilder, Stroke, SurfaceId, SurfaceView, Target, TargetKind,
};
use artic_vello::Renderer;
use common::with_renderer;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

const SIZE: u32 = 16;
const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);
const CLEAR: [u8; 4] = [0, 0, 0, 0];
const OPAQUE_RED: [u8; 4] = [255, 0, 0, 255];
const BLACK: Color = Color::rgb(0, 0, 0);
const OPAQUE_BLACK: [u8; 4] = [0, 0, 0, 255];
const AWAY: f32 = 64.0;

#[derive(Debug)]
struct Node;

impl Object for Node {
  fn visit_properties(&self, _visit: &mut dyn FnMut(AnyProperty<'_>)) {}
}

type Draw = Box<dyn Fn(&mut SceneBuilder)>;

/// Each node draws with its own closure, inside optional layers.
#[derive(Default)]
struct Drawings {
  draws: HashMap<NodeId, Draw>,
  layers: HashMap<NodeId, Layers>,
}

impl Nodes for Drawings {
  fn layers(&self, _arena: &UiTree, node: NodeId) -> Layers {
    self.layers.get(&node).cloned().unwrap_or_default()
  }

  fn record(&mut self, _arena: &UiTree, node: NodeId, builder: &mut SceneBuilder) {
    if let Some(draw) = self.draws.get(&node) {
      draw(builder);
    }
  }
}

fn fill(rect: Rect, color: Color) -> Draw {
  Box::new(move |builder| builder.fill(rect, color))
}

/// Renders `root`'s tree into an `SIZE`×`SIZE` buffer of `format`, and returns its pixels.
fn render(renderer: &mut Renderer, tree: &mut UiTree, root: NodeId, nodes: &mut Drawings, format: PixelFormat) -> Vec<[u8; 4]> {
  let scene = Compositor::default().compose(tree, root, nodes);
  let mut data = vec![0; (SIZE * SIZE * 4) as usize];
  let surface = SurfaceView {
    id: SurfaceId::new(1),
    size: PhysicalSize::new(SIZE, SIZE),
    scale: 1.0,
    target: Target::Pixels(PixelBuffer {
      data: &mut data,
      stride: (SIZE * 4) as usize,
      format,
    }),
  };
  renderer.render(&scene, surface).expect("renders");
  data.as_chunks::<4>().0.to_vec()
}

fn pixel(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
  pixels[(y * SIZE + x) as usize]
}

fn full() -> Rect {
  Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32)
}

fn render_one(renderer: &mut Renderer, draw: impl Fn(&mut SceneBuilder) + 'static) -> Vec<[u8; 4]> {
  let mut tree = UiTree::new();
  let root = tree.alloc(Box::new(Node));
  let mut nodes = Drawings::default();
  nodes.draws.insert(root, Box::new(draw));
  render(renderer, &mut tree, root, &mut nodes, PixelFormat::Rgba8)
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], what: &str) {
  let near = actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2);
  assert!(near, "{what}: got {actual:?}, expected {expected:?}");
}

// A black shadow that lands on `rect`, cast from `AWAY` to its left so the caster is out of sight.
fn shadow_on(builder: &mut SceneBuilder, rect: impl Into<RoundedRect>, blur: f32) {
  let RoundedRect { rect, radius } = rect.into();
  let caster = RoundedRect::new(Rect::new(rect.x - AWAY, rect.y, rect.width, rect.height), radius);
  builder.box_shadow(caster, BoxShadow::new(Point::new(AWAY, 0.0), blur, BLACK));
}

// Red up to 40%, blue from 60%: pixels near either end are exactly one color.
fn red_to_blue() -> [GradientStop; 4] {
  [
    GradientStop::new(0.0, RED),
    GradientStop::new(0.4, RED),
    GradientStop::new(0.6, BLUE),
    GradientStop::new(1.0, BLUE),
  ]
}

// A filled scene reads back exactly, in RGBA and in BGRA order.
#[test]
fn renders_scene_into_pixels() {
  with_renderer(|renderer| {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Node));
    let mut nodes = Drawings::default();
    nodes.draws.insert(root, fill(full(), RED));

    let rgba = render(renderer, &mut tree, root, &mut nodes, PixelFormat::Rgba8);
    tree.mark_dirty(root, artic_core::property::Affects::RENDER);
    let bgra = render(renderer, &mut tree, root, &mut nodes, PixelFormat::Bgra8);

    assert!(rgba.iter().all(|pixel| *pixel == [255, 0, 0, 255]), "{rgba:?}");
    assert!(bgra.iter().all(|pixel| *pixel == [0, 0, 255, 255]), "{bgra:?}");
  });
}

// A child draws over its parent, moved by the parent's transform and cut by the parent's clip.
#[test]
fn transform_and_clip_place_the_child() {
  with_renderer(|renderer| {
    let mut tree = UiTree::new();
    let [root, panel, child] = [(); 3].map(|()| tree.alloc(Box::new(Node)));
    tree.add_child(TreeKind::Visual, root, panel);
    tree.add_child(TreeKind::Visual, panel, child);
    let mut nodes = Drawings::default();
    nodes.draws.insert(root, fill(full(), BLUE));
    nodes.draws.insert(child, fill(Rect::new(0.0, 0.0, 4.0, 4.0), GREEN));
    nodes.layers.insert(
      panel,
      Layers {
        transform: Some(Transform::translate(4.0, 4.0)),
        clip: Some(Rect::new(0.0, 0.0, 2.0, 4.0).into()),
        ..Layers::default()
      },
    );

    let pixels = render(renderer, &mut tree, root, &mut nodes, PixelFormat::Rgba8);

    assert_eq!(pixel(&pixels, 1, 1), [0, 0, 255, 255], "outside the moved child");
    assert_eq!(pixel(&pixels, 5, 5), [0, 255, 0, 255], "inside the child and its clip");
    assert_eq!(pixel(&pixels, 7, 5), [0, 0, 255, 255], "cut off by the clip");
  });
}

// An opacity layer blends its content over what's below.
#[test]
fn opacity_blends_the_layer() {
  with_renderer(|renderer| {
    let mut tree = UiTree::new();
    let [root, overlay] = [(); 2].map(|()| tree.alloc(Box::new(Node)));
    tree.add_child(TreeKind::Visual, root, overlay);
    let mut nodes = Drawings::default();
    nodes.draws.insert(root, fill(full(), Color::rgb(0, 0, 0)));
    nodes.draws.insert(overlay, fill(full(), Color::rgb(255, 255, 255)));
    nodes.layers.insert(
      overlay,
      Layers {
        opacity: 0.5,
        ..Layers::default()
      },
    );

    let pixels = render(renderer, &mut tree, root, &mut nodes, PixelFormat::Rgba8);

    let [r, g, b, a] = pixel(&pixels, 3, 3);
    assert!(r.abs_diff(128) <= 2 && r == g && g == b, "half-white grey, got {r} {g} {b}");
    assert_eq!(a, 255);
  });
}

// A left-to-right gradient is its first stop at the left edge and its last at the right.
#[test]
fn linear_gradient_fills_rect() {
  with_renderer(|renderer| {
    let brush = LinearGradientBrush::new(
      RelativePoint::relative(0.0, 0.0),
      RelativePoint::relative(1.0, 0.0),
      red_to_blue(),
    );

    let pixels = render_one(renderer, move |builder| builder.fill(full(), brush.clone()));

    for y in 0..SIZE {
      assert_near(pixel(&pixels, 0, y), [255, 0, 0, 255], "left edge");
      assert_near(pixel(&pixels, SIZE - 1, y), [0, 0, 255, 255], "right edge");
    }
    let [r, _, b, _] = pixel(&pixels, SIZE / 2, 0);
    assert!(r > 2 && b > 2, "the middle blends, got red {r} and blue {b}");
  });
}

// In a box twice as wide as it is tall, radii of 50% make an ellipse: its center is the first
// stop, and the ends of both radii are the last. A circle of either radius would miss one end.
// Vello samples gradients at a pixel's corner, not its center; these pixels hold either way.
#[test]
fn radial_gradient_fills_an_ellipse() {
  with_renderer(|renderer| {
    let wide = Rect::new(0.0, 0.0, 8.0, 4.0);
    let brush = RadialGradientBrush {
      stops: red_to_blue().into(),
      ..RadialGradientBrush::default()
    };

    let pixels = render_one(renderer, move |builder| builder.fill(wide, brush.clone()));

    assert_near(pixel(&pixels, 4, 2), [255, 0, 0, 255], "center");
    assert_near(pixel(&pixels, 7, 2), [0, 0, 255, 255], "end of the horizontal radius");
    assert_near(pixel(&pixels, 4, 0), [0, 0, 255, 255], "end of the vertical radius");
  });
}

// Radii cut the corners they name, clockwise from the top-left; square corners and the middle
// stay filled.
#[test]
fn rounded_rect_leaves_corners_clear() {
  with_renderer(|renderer| {
    let card = RoundedRect::new(full(), CornerRadius::new(8.0, 0.0, 8.0, 0.0));
    let last = SIZE - 1;

    let pixels = render_one(renderer, move |builder| builder.fill(card, RED));

    assert_near(pixel(&pixels, 0, 0), CLEAR, "top-left, rounded");
    assert_near(pixel(&pixels, last, last), CLEAR, "bottom-right, rounded");
    assert_near(pixel(&pixels, last, 0), OPAQUE_RED, "top-right, square");
    assert_near(pixel(&pixels, 0, last), OPAQUE_RED, "bottom-left, square");
    assert_near(pixel(&pixels, 8, 8), OPAQUE_RED, "middle");
  });
}

// A triangle clip keeps what's inside it and hides the rest.
#[test]
fn path_clip_masks_drawing() {
  with_renderer(|renderer| {
    let triangle = Path::builder(Point::new(0.0, 0.0))
      .line_to(Point::new(16.0, 0.0))
      .line_to(Point::new(0.0, 16.0))
      .close()
      .build();

    let pixels = render_one(renderer, move |builder| {
      builder.with_clip(triangle.clone(), |builder| builder.fill(full(), RED));
    });

    assert_near(pixel(&pixels, 2, 2), OPAQUE_RED, "inside the triangle");
    assert_near(pixel(&pixels, 13, 13), CLEAR, "outside it");
  });
}

fn star(fill_rule: FillRule) -> Path {
  let corner = |index: u16| {
    let angle = (f32::from(index) * 144.0 - 90.0).to_radians();
    Point::new(8.0 + 8.0 * angle.cos(), 8.0 + 8.0 * angle.sin())
  };
  (1..5)
    .fold(Path::builder(corner(0)), |builder, index| builder.line_to(corner(index)))
    .close()
    .fill_rule(fill_rule)
    .build()
}

// The outline of a star winds twice around its center: the even-odd rule leaves the center
// empty, the non-zero rule fills it. Both fill the points.
#[test]
fn fill_rule_decides_a_star_center() {
  with_renderer(|renderer| {
    let even_odd = render_one(renderer, |builder| builder.fill(star(FillRule::EvenOdd), RED));
    let non_zero = render_one(renderer, |builder| builder.fill(star(FillRule::NonZero), RED));

    assert_near(pixel(&even_odd, 7, 7), CLEAR, "even-odd center");
    assert_near(pixel(&non_zero, 7, 7), OPAQUE_RED, "non-zero center");
    assert_near(pixel(&even_odd, 7, 4), OPAQUE_RED, "even-odd top point");
    assert_near(pixel(&non_zero, 7, 4), OPAQUE_RED, "non-zero top point");
  });
}

// A gradient on a path spans the path's own box: red at its left edge, blue at its right, though
// the box starts halfway across the surface.
#[test]
fn gradient_on_path_uses_its_bounds() {
  with_renderer(|renderer| {
    let square = Path::builder(Point::new(8.0, 0.0))
      .line_to(Point::new(16.0, 0.0))
      .line_to(Point::new(16.0, 8.0))
      .line_to(Point::new(8.0, 8.0))
      .close()
      .build();
    let brush = LinearGradientBrush::new(
      RelativePoint::relative(0.0, 0.0),
      RelativePoint::relative(1.0, 0.0),
      red_to_blue(),
    );

    let pixels = render_one(renderer, move |builder| builder.fill(square.clone(), brush.clone()));

    assert_near(pixel(&pixels, 8, 4), [255, 0, 0, 255], "the path's left edge");
    assert_near(pixel(&pixels, 15, 4), [0, 0, 255, 255], "the path's right edge");
    assert_near(pixel(&pixels, 4, 4), CLEAR, "outside the path");
  });
}

// A clip whose shape draws nothing hides what it wraps, and what's drawn after it is unaffected.
#[test]
fn undrawable_clip_hides_its_content() {
  with_renderer(|renderer| {
    let pixels = render_one(renderer, |builder| {
      builder.with_clip(Rect::new(0.0, 0.0, f32::NAN, 16.0), |builder| builder.fill(full(), RED));
      builder.fill(Rect::new(0.0, 0.0, 4.0, 4.0), BLUE);
    });

    assert_near(pixel(&pixels, 8, 8), CLEAR, "inside the broken clip");
    assert_near(pixel(&pixels, 1, 1), [0, 0, 255, 255], "drawn after it");
  });
}

// A fill closes an open figure with a straight line back to its start.
#[test]
fn open_figure_fills_closed() {
  with_renderer(|renderer| {
    let open = Path::builder(Point::new(0.0, 0.0))
      .line_to(Point::new(16.0, 0.0))
      .line_to(Point::new(0.0, 16.0))
      .build();

    let pixels = render_one(renderer, move |builder| builder.fill(open.clone(), RED));

    assert_near(pixel(&pixels, 2, 2), OPAQUE_RED, "inside the closed triangle");
    assert_near(pixel(&pixels, 13, 13), CLEAR, "outside it");
  });
}

// A stroke 2 wide on a rectangle's edges paints a band on each side of every edge, with a full
// miter at the corners, and leaves the inside empty.
#[test]
fn stroke_paints_only_the_outline() {
  with_renderer(|renderer| {
    let pixels = render_one(renderer, |builder| {
      builder.stroke(Rect::new(1.0, 1.0, 14.0, 14.0), RED, Stroke::new(2.0));
    });

    for (x, what) in [
      (0, "outside the left edge"),
      (1, "inside the left edge"),
      (14, "inside the right edge"),
      (15, "outside the right edge"),
    ] {
      assert_near(pixel(&pixels, x, 8), OPAQUE_RED, what);
    }
    assert_near(pixel(&pixels, 0, 0), OPAQUE_RED, "the mitered corner");
    assert_near(pixel(&pixels, 2, 8), CLEAR, "just inside the band");
    assert_near(pixel(&pixels, 8, 8), CLEAR, "the middle");
  });
}

// A stroke leaves an open figure open: no line runs back from its end to its start.
#[test]
fn stroke_leaves_open_figures_open() {
  with_renderer(|renderer| {
    let corner = Path::builder(Point::new(2.0, 2.0))
      .line_to(Point::new(14.0, 2.0))
      .line_to(Point::new(14.0, 14.0))
      .build();

    let pixels = render_one(renderer, move |builder| builder.stroke(corner.clone(), RED, Stroke::new(2.0)));

    assert_near(pixel(&pixels, 8, 1), OPAQUE_RED, "on the first segment");
    assert_near(pixel(&pixels, 14, 8), OPAQUE_RED, "on the second segment");
    assert_near(pixel(&pixels, 8, 8), CLEAR, "where a closing line would run");
  });
}

// A blurred shadow is darkest in its middle and fades past its edges. A shadow without blur ends
// at its rectangle, so its edges fall where a fill's would: vello's blur reads each pixel at its
// corner, and the translation moves the rectangle back half a pixel to read at the centers.
#[test]
fn box_shadow_fades_outside_the_rect() {
  with_renderer(|renderer| {
    let card = Rect::new(4.0, 4.0, 8.0, 8.0);

    let blurred = render_one(renderer, move |builder| {
      shadow_on(builder, card, 4.0);
    });
    let sharp = render_one(renderer, move |builder| {
      shadow_on(builder, card, 0.0);
    });

    let fade = [8, 4, 3, 2, 0].map(|x| pixel(&blurred, x, 8)[3]);
    assert!(
      fade.windows(2).all(|pair| pair[0] > pair[1]),
      "fades from the middle out: {fade:?}"
    );
    assert!(fade[4] > 0, "the blur reaches past the edge: {fade:?}");
    assert_near(pixel(&sharp, 4, 8), OPAQUE_BLACK, "sharp, inside");
    assert_near(pixel(&sharp, 11, 11), OPAQUE_BLACK, "sharp, last corner pixel");
    assert_near(pixel(&sharp, 3, 8), CLEAR, "sharp, outside");
    assert_near(pixel(&sharp, 12, 12), CLEAR, "sharp, past the corner");
  });
}

// A shadow with unequal radii draws each quarter as a shadow with that quarter's radius on every
// corner would. The rectangle's middle falls mid-pixel, and no seam shows where the quarters meet.
#[test]
fn unequal_radii_shadow_matches_each_corner() {
  with_renderer(|renderer| {
    let rect = Rect::new(2.0, 2.0, 11.0, 12.0);
    let radii = CornerRadius::new(0.0, 2.0, 4.0, 5.0);
    let shadow_of = move |radius| {
      move |builder: &mut SceneBuilder| {
        shadow_on(builder, RoundedRect::new(rect, radius), 2.0);
      }
    };

    let mixed = render_one(renderer, shadow_of(radii));

    let quarters = [
      (0..8, 0..8, radii.top_left),
      (8..SIZE, 0..8, radii.top_right),
      (8..SIZE, 8..SIZE, radii.bottom_right),
      (0..8, 8..SIZE, radii.bottom_left),
    ];
    for (xs, ys, radius) in quarters {
      let uniform = render_one(renderer, shadow_of(CornerRadius::uniform(radius)));
      for y in ys {
        for x in xs.clone() {
          assert_near(
            pixel(&mixed, x, y),
            pixel(&uniform, x, y),
            &format!("radius {radius} at {x},{y}"),
          );
        }
      }
    }
  });
}

// A shadow without blur is the rounded rectangle filled: its curves and its edges between pixels
// are anti-aliased as a fill's are.
#[test]
fn sharp_shadow_draws_like_a_fill() {
  with_renderer(|renderer| {
    let card = RoundedRect::new(Rect::new(2.3, 2.7, 10.0, 10.0), CornerRadius::new(4.0, 0.0, 2.0, 5.0));

    let shadow = render_one(renderer, move |builder| {
      shadow_on(builder, card, 0.0);
    });
    let fill = render_one(renderer, move |builder| builder.fill(card, BLACK));

    for y in 0..SIZE {
      for x in 0..SIZE {
        assert_near(pixel(&shadow, x, y), pixel(&fill, x, y), &format!("{x},{y}"));
      }
    }
  });
}

// The quarters of a shadow with unequal radii add up where they meet, so no seam shows inside it
// even when a rotation cuts the pixels along the seam.
#[test]
fn rotated_unequal_radii_shadow_has_no_seam() {
  with_renderer(|renderer| {
    let card = RoundedRect::new(Rect::new(-6.0, -6.0, 12.0, 12.0), CornerRadius::new(0.0, 2.0, 4.0, 6.0));
    let turned = Transform::rotate(0.5).then(Transform::translate(8.0, 8.0));

    let pixels = render_one(renderer, move |builder| {
      builder.with_transform(turned, |builder| {
        shadow_on(builder, card, 1.0);
      });
    });

    for y in 5..11 {
      for x in 5..11 {
        assert_near(pixel(&pixels, x, y), OPAQUE_BLACK, &format!("{x},{y}"));
      }
    }
  });
}

// A shadow shows only outside the rectangle that casts it, so a translucent caster above it keeps
// its own color.
#[test]
fn box_shadow_stays_outside_its_caster() {
  with_renderer(|renderer| {
    let card = Rect::new(4.0, 4.0, 6.0, 6.0);

    for blur in [0.0, 1.0] {
      let pixels = render_one(renderer, move |builder| {
        builder.box_shadow(card, BoxShadow::new(Point::new(3.0, 3.0), blur, BLACK));
      });

      assert_near(pixel(&pixels, 8, 8), CLEAR, &format!("blur {blur}, under the caster"));
      assert_near(pixel(&pixels, 11, 11), OPAQUE_BLACK, &format!("blur {blur}, past the caster"));
    }
  });
}

// A shadow is drawn in the coordinates of the transform around it.
#[test]
fn box_shadow_follows_the_transform() {
  with_renderer(|renderer| {
    let pixels = render_one(renderer, |builder| {
      builder.with_transform(Transform::translate(8.0, 0.0), |builder| {
        shadow_on(builder, Rect::new(0.0, 4.0, 4.0, 4.0), 0.0);
      });
    });

    assert_near(pixel(&pixels, 9, 5), OPAQUE_BLACK, "moved");
    assert_near(pixel(&pixels, 1, 5), CLEAR, "not where it was");
  });
}

// The renderer draws on windows and pixel buffers, and rejects the rest.
#[test]
fn accepts_handles_and_pixels() {
  with_renderer(|renderer| {
    assert!(renderer.accepts(TargetKind::Handles) && renderer.accepts(TargetKind::Pixels));
    assert!(!renderer.accepts(TargetKind::Custom));
    let mut other = ();
    let surface = SurfaceView {
      id: SurfaceId::new(1),
      size: PhysicalSize::new(1, 1),
      scale: 1.0,
      target: Target::Custom(&mut other),
    };
    let error = renderer.render(&Default::default(), surface).expect_err("custom");
    assert!(matches!(error, RenderError::Unsupported(TargetKind::Custom)));
  });
}

/// `text` shaped at 32 px with only the test font's regular face, in `weight` and `style`.
fn shaped(text: &str, weight: FontWeight, style: FontStyle) -> Paragraph {
  let fonts = [Arc::new(artic_test_fonts::REGULAR) as Arc<dyn AsRef<[u8]> + Send + Sync>];
  let style = TextStyle {
    weight,
    style,
    ..TextStyle::new(FontFamily::default(), 32.0)
  };
  artic_text::Engine::with_fonts(fonts).paragraph(text, &style, &ParagraphStyle::default(), TextLimits::NONE)
}

/// The alpha of each pixel of `paragraph` drawn in black at (8, 8) on 64 by 64, row by row.
fn coverage(renderer: &mut Renderer, paragraph: &Paragraph) -> Vec<u32> {
  let scene = artic_test::scenes::scene(|builder| builder.text(paragraph, Point::new(8.0, 8.0), BLACK));
  let image = artic_test::golden::render(renderer, &scene, PhysicalSize::new(64, 64), 1.0).expect("renders");
  image.pixels().iter().map(|pixel| u32::from(pixel[3])).collect()
}

// Bold asked of a face without a bold one is drawn thicker: more of the image is covered.
#[test]
fn synthesized_bold_draws_heavier() {
  with_renderer(|renderer| {
    let regular: u32 = coverage(renderer, &shaped("Hi", FontWeight::NORMAL, FontStyle::Normal))
      .iter()
      .sum();
    let bold: u32 = coverage(renderer, &shaped("Hi", FontWeight::BOLD, FontStyle::Normal))
      .iter()
      .sum();

    assert!(bold > regular * 11 / 10, "bold covers {bold}, regular {regular}");
  });
}

// Italic asked of a face without an italic one leans right: the ink of the top rows sits further
// right than the ink of the bottom rows.
#[test]
fn synthesized_italic_leans_right() {
  with_renderer(|renderer| {
    let alpha = coverage(renderer, &shaped("l", FontWeight::NORMAL, FontStyle::Italic));
    let center = |rows: Range<usize>| {
      let (mut weighted, mut total) = (0.0, 0.0);
      for row in rows {
        for column in 0..64 {
          let a = alpha[row * 64 + column] as f32;
          weighted += a * column as f32;
          total += a;
        }
      }
      weighted / total
    };

    assert!(
      center(10..22) > center(30..42) + 1.0,
      "top {}, bottom {}",
      center(10..22),
      center(30..42)
    );
  });
}
