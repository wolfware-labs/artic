// The pipeline for one surface, without a host: layout, composition and presenting.

mod common;

use artic_core::tree::{TreeKind, UiTree};
use artic_element::{ElementTypes, SizeToContent};
use artic_layout::geometry::{Size, Transform};
use artic_platform::{Axes, Frame, FrameServices, FrameStats, Pipeline};
use artic_render::{Layer, SceneItem, SurfaceId};
use common::{Counter, Label, Recorder, column, engine, info, label, leaf, pixels, target};
use pretty_assertions::assert_eq;
use static_assertions::assert_impl_all;

assert_impl_all!(Frame: Send);

const SURFACE: SurfaceId = SurfaceId::new(1);

// A node's own transform applies first, in its own coordinates, and its layout offset after it.
#[test]
fn layout_offset_applies_after_render_transform() {
  let mut tree = UiTree::new();
  let top = leaf(&mut tree, 20.0);
  let turned = leaf(&mut tree, 10.0);
  let (root, _) = column(&mut tree, &[top.id, turned.id]);
  turned.turn.set(true);

  let frame = Pipeline::new().prepare(
    &mut tree,
    root,
    SURFACE,
    &info(64, 64, 1.0),
    &mut FrameServices {
      types: &ElementTypes::from_link_time(),
      text: &mut engine(),
    },
    Axes::NONE,
  );

  let expected = Transform::scale(2.0, 2.0).then(Transform::translate(0.0, 20.0));
  assert!(frame.scene().items().contains(&SceneItem::Push(Layer::Transform(expected))));
}

// An element draws with the size layout arranged it at, in logical units.
#[test]
fn render_cx_has_the_arranged_size() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 20.0);
  let (root, _) = column(&mut tree, &[child.id]);

  Pipeline::new().prepare(
    &mut tree,
    root,
    SURFACE,
    &info(128, 128, 2.0),
    &mut FrameServices {
      types: &ElementTypes::from_link_time(),
      text: &mut engine(),
    },
    Axes::NONE,
  );

  let seen = tree.get_as::<common::Leaf>(child.id).unwrap().seen.get();
  assert_eq!(seen, Some(Size::new(64.0, 20.0)));
}

// A first frame lays out and records every node, and presenting it marks it rendered.
#[test]
fn first_frame_does_all_the_work() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 20.0);
  let (root, _) = column(&mut tree, &[child.id]);
  let mut pipeline = Pipeline::new();
  let mut buffer = pixels();

  let frame = pipeline.prepare(
    &mut tree,
    root,
    SURFACE,
    &info(64, 64, 1.0),
    &mut FrameServices {
      types: &ElementTypes::from_link_time(),
      text: &mut engine(),
    },
    Axes::NONE,
  );
  let stats = pipeline.present(&mut Counter::default(), frame, target(&mut buffer)).unwrap();

  assert_eq!(
    stats,
    FrameStats {
      measured: 2,
      arranged: 2,
      recorded: 2,
      rendered: true
    }
  );
}

// Redrawing presents the last scene again without layout or recording, and there's nothing to
// redraw before the first frame.
#[test]
fn redraw_presents_the_last_scene() {
  let mut tree = UiTree::new();
  let (root, _) = column(&mut tree, &[]);
  let mut pipeline = Pipeline::new();
  let mut renderer = Counter::default();
  let mut buffer = pixels();
  assert!(
    pipeline
      .redraw(&mut renderer, SURFACE, &info(64, 64, 1.0), target(&mut buffer))
      .is_none()
  );
  let frame = pipeline.prepare(
    &mut tree,
    root,
    SURFACE,
    &info(64, 64, 1.0),
    &mut FrameServices {
      types: &ElementTypes::from_link_time(),
      text: &mut engine(),
    },
    Axes::NONE,
  );
  pipeline.present(&mut renderer, frame, target(&mut buffer)).unwrap();

  let stats = pipeline
    .redraw(&mut renderer, SURFACE, &info(64, 64, 1.0), target(&mut buffer))
    .unwrap()
    .unwrap();

  assert_eq!(
    stats,
    FrameStats {
      rendered: true,
      ..FrameStats::default()
    }
  );
  assert_eq!(renderer.rendered.get(), 2);
}

// A scene the renderer failed to draw isn't kept for redraws: only a presented scene is.
#[test]
fn failed_present_keeps_no_scene() {
  let mut tree = UiTree::new();
  let (root, _) = column(&mut tree, &[]);
  let mut pipeline = Pipeline::new();
  let mut renderer = Counter::default();
  *renderer.fail.borrow_mut() = Some(artic_render::RenderError::SurfaceLost);
  let mut buffer = pixels();
  let frame = pipeline.prepare(
    &mut tree,
    root,
    SURFACE,
    &info(64, 64, 1.0),
    &mut FrameServices {
      types: &ElementTypes::from_link_time(),
      text: &mut engine(),
    },
    Axes::NONE,
  );

  assert!(pipeline.present(&mut renderer, frame, target(&mut buffer)).is_err());

  assert!(
    pipeline
      .redraw(&mut renderer, SURFACE, &info(64, 64, 1.0), target(&mut buffer))
      .is_none()
  );
}

// A surface measured before its window opens, in the size its content axes get, answers the
// first frame's measures from the cache, and the frame lays it out at its desired size there.
#[test]
fn measure_before_open_serves_the_first_frame() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 20.0);
  let (root, _) = column(&mut tree, &[child.id]);
  let types = ElementTypes::from_link_time();
  let mut text = engine();
  let mut services = FrameServices {
    types: &types,
    text: &mut text,
  };
  let mut pipeline = Pipeline::new();

  let desired = pipeline.measure(&mut tree, root, Size::new(f32::INFINITY, f32::INFINITY), &mut services);
  let frame = pipeline.prepare(&mut tree, root, SURFACE, &info(64, 64, 1.0), &mut services, Axes::BOTH);

  assert_eq!(desired, Size::new(10.0, 20.0));
  assert_eq!(pipeline.desired(root), Some(desired));
  assert_eq!(frame.stats().measured, 0);
}

// A SizeToContent value names the axes content sizes.
#[test]
fn axes_from_size_to_content() {
  assert_eq!(
    Axes::from(SizeToContent::Height),
    Axes {
      width: false,
      height: true
    }
  );
  assert_eq!(Axes::from(SizeToContent::WidthAndHeight), Axes::BOTH);
  assert_eq!(Axes::from(SizeToContent::Manual), Axes::NONE);
}

// A node layout left out isn't drawn, and neither is its subtree.
#[test]
fn nodes_not_laid_out_are_not_drawn() {
  let mut tree = UiTree::new();
  let shown = leaf(&mut tree, 20.0);
  let hidden = leaf(&mut tree, 20.0);
  let under = leaf(&mut tree, 20.0);
  tree.add_child(TreeKind::Visual, hidden.id, under.id);
  tree.get_as::<common::Leaf>(hidden.id).unwrap().collapsed.set(true);
  let (root, _) = column(&mut tree, &[shown.id, hidden.id]);

  Pipeline::new().prepare(
    &mut tree,
    root,
    SURFACE,
    &info(64, 64, 1.0),
    &mut FrameServices {
      types: &ElementTypes::from_link_time(),
      text: &mut engine(),
    },
    Axes::NONE,
  );

  let seen = |id| tree.get_as::<common::Leaf>(id).unwrap().seen.get();
  assert_eq!(
    (seen(shown.id), seen(hidden.id), seen(under.id)),
    (Some(Size::new(64.0, 20.0)), None, None)
  );
}

// A node whose layout result was replaced is recorded again, even when its size stays the same.
#[test]
fn replaced_result_is_recorded_again() {
  let mut tree = UiTree::new();
  let (node, text) = label(&mut tree, "short");
  let (root, _) = column(&mut tree, &[node]);
  let types = ElementTypes::from_link_time();
  let mut engine = Recorder::default();
  let mut pipeline = Pipeline::new();
  pipeline.prepare(
    &mut tree,
    root,
    SURFACE,
    &info(256, 64, 1.0),
    &mut FrameServices {
      types: &types,
      text: &mut engine,
    },
    Axes::NONE,
  );
  tree.get_as::<Label>(node).unwrap().drawn.set(None);
  text.set("words".to_owned());

  let stats = pipeline
    .prepare(
      &mut tree,
      root,
      SURFACE,
      &info(256, 64, 1.0),
      &mut FrameServices {
        types: &types,
        text: &mut engine,
      },
      Axes::NONE,
    )
    .stats();

  assert!(stats.recorded >= 1);
  let drawn = tree.get_as::<Label>(node).unwrap().drawn.get();
  assert_eq!(drawn, Some(Size::new(8.0 * 5.0, 16.0)));
}
