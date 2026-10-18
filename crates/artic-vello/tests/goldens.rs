// Draws each scene of `artic_test::scenes` and checks it against its PNG in `tests/goldens`.
// `ARTIC_UPDATE_GOLDENS=1` rewrites the PNGs. Tests skip without a usable GPU, unless
// `ARTIC_REQUIRE_GPU=1`.

mod common;

use artic_controls::{Border, StackPanel};
use artic_core::tree::UiTree;
use artic_element::{Child, Children, ElementTypes};
use artic_layout::HorizontalAlignment;
use artic_layout::geometry::{CornerRadius, Point, Thickness};
use artic_render::{BoxShadow, Color, Scene};
use artic_test::golden::{render, render_tree};
use artic_test::{golden, scenes};
use common::{is_warp, with_renderer};
use vello::wgpu::{Backend, DeviceType};

fn check(name: &str, scene: fn() -> Scene) {
  with_renderer(|renderer| {
    let image = render(renderer, &scene(), scenes::SIZE, 1.0).expect("renders");
    golden!(name).assert(&image);
  });
}

// WARP is a CPU adapter on DX12; lavapipe, a CPU adapter on Vulkan, and real GPUs are usable.
#[test]
fn only_dx12_cpu_adapters_are_warp() {
  assert!(is_warp(Backend::Dx12, DeviceType::Cpu));
  assert!(!is_warp(Backend::Vulkan, DeviceType::Cpu));
  assert!(!is_warp(Backend::Dx12, DeviceType::DiscreteGpu));
}

// A rounded rectangle with four different radii.
#[test]
fn rounded_rect_fill() {
  check("rounded_rect_fill", scenes::rounded_rect_fill);
}

// A star stroked with round joins.
#[test]
fn stroke() {
  check("stroke", scenes::stroke);
}

// A star filled even-odd.
#[test]
fn path() {
  check("path", scenes::path);
}

// A diagonal linear gradient of three stops.
#[test]
fn linear_gradient() {
  check("linear_gradient", scenes::linear_gradient);
}

// An elliptical radial gradient.
#[test]
fn radial_gradient() {
  check("radial_gradient", scenes::radial_gradient);
}

// A translucent card over its shadow.
#[test]
fn box_shadow() {
  check("box_shadow", scenes::box_shadow);
}

// "Text" in solid blue.
#[test]
fn text_solid() {
  check("text_solid", scenes::text_solid);
}

// Two lines painted with one gradient across the paragraph.
#[test]
fn text_gradient() {
  check("text_gradient", scenes::text_gradient);
}

// A sentence wrapped to the width.
#[test]
fn text_wrapped() {
  check("text_wrapped", scenes::text_wrapped);
}

// A gradient clipped by a rectangle.
#[test]
fn rect_clip() {
  check("rect_clip", scenes::rect_clip);
}

// A gradient clipped by a circle.
#[test]
fn rounded_rect_clip() {
  check("rounded_rect_clip", scenes::rounded_rect_clip);
}

// A gradient clipped by a star.
#[test]
fn path_clip() {
  check("path_clip", scenes::path_clip);
}

// Two squares in nested opacity layers.
#[test]
fn nested_opacity() {
  check("nested_opacity", scenes::nested_opacity);
}

// A vertical stack panel with a light background and three fixed-size children, each a stack panel
// with its own color, 4 apart.
#[test]
fn stack_panel_background() {
  with_renderer(|renderer| {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(StackPanel::default()));
    let stack = tree.get_as::<StackPanel>(root).expect("a stack panel");
    stack.panel.background.set(Some(Color::rgb(240, 240, 240).into()));
    stack.spacing.set(4.0);
    let bars = [
      (40.0, Color::rgb(220, 60, 60)),
      (24.0, Color::rgb(60, 160, 60)),
      (56.0, Color::rgb(60, 90, 220)),
    ];
    for (width, color) in bars {
      let child = tree.alloc(Box::new(StackPanel::default()));
      let bar = tree.get_as::<StackPanel>(child).expect("a stack panel");
      bar.layout.width.set(Some(width));
      bar.layout.height.set(Some(12.0));
      bar.layout.horizontal_alignment.set(HorizontalAlignment::Left);
      bar.panel.background.set(Some(color.into()));
      Children::add(&mut tree, root, child);
    }
    let image = render_tree(renderer, &mut tree, root, &ElementTypes::from_link_time(), scenes::SIZE, 1.0).expect("renders");
    golden!("stack_panel_background").assert(&image);
  });
}

fn border_golden(name: &str, configure: impl FnOnce(&Border), child: Option<Color>) {
  with_renderer(|renderer| {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Border::default()));
    let border = tree.get_as::<Border>(root).expect("a border");
    border.layout.margin.set(Thickness::uniform(12.0));
    configure(border);
    if let Some(color) = child {
      let leaf = tree.alloc(Box::new(StackPanel::default()));
      tree
        .get_as::<StackPanel>(leaf)
        .expect("a stack panel")
        .panel
        .background
        .set(Some(color.into()));
      Child::set(&mut tree, root, Some(leaf));
    }
    let image = render_tree(renderer, &mut tree, root, &ElementTypes::from_link_time(), scenes::SIZE, 1.0).expect("renders");
    golden!(name).assert(&image);
  });
}

// A 40 by 40 box filled light gray with an even 2-wide dark outline.
#[test]
fn border_background() {
  border_golden(
    "border_background",
    |border| {
      border.background.set(Some(Color::rgb(230, 230, 230).into()));
      border.border_brush.set(Some(Color::rgb(40, 40, 40).into()));
      border.border_thickness.set(Thickness::uniform(2.0));
    },
    None,
  );
}

// Sides of 1, 4, 8 and 2 around a white box.
#[test]
fn border_uneven_thickness() {
  border_golden(
    "border_uneven_thickness",
    |border| {
      border.background.set(Some(Color::rgb(255, 255, 255).into()));
      border.border_brush.set(Some(Color::rgb(60, 90, 220).into()));
      border.border_thickness.set(Thickness::new(1.0, 4.0, 8.0, 2.0));
    },
    None,
  );
}

// Four different corner radii with a 3-wide outline whose inner corners follow them.
#[test]
fn border_corner_radius() {
  border_golden(
    "border_corner_radius",
    |border| {
      border.background.set(Some(Color::rgb(250, 240, 200).into()));
      border.border_brush.set(Some(Color::rgb(200, 120, 0).into()));
      border.border_thickness.set(Thickness::uniform(3.0));
      border.corner_radius.set(CornerRadius::new(16.0, 4.0, 10.0, 0.0));
    },
    None,
  );
}

// A rounded white card with a soft shadow below it.
#[test]
fn border_box_shadow() {
  border_golden(
    "border_box_shadow",
    |border| {
      border.background.set(Some(Color::rgb(255, 255, 255).into()));
      border.corner_radius.set(CornerRadius::uniform(8.0));
      border
        .box_shadow
        .set(Some(BoxShadow::new(Point::new(0.0, 2.0), 8.0, Color::rgba(0, 0, 0, 96))));
    },
    None,
  );
}

// A red child filling the inside: its corners are cut by the rounded outline.
#[test]
fn border_clips_child() {
  border_golden(
    "border_clips_child",
    |border| {
      border.border_brush.set(Some(Color::rgb(40, 40, 40).into()));
      border.border_thickness.set(Thickness::uniform(2.0));
      border.corner_radius.set(CornerRadius::uniform(14.0));
    },
    Some(Color::rgb(220, 60, 60)),
  );
}
