//! Compiles the Rust code blocks in the repository's documents as doctests, so a document that
//! names a renamed or removed API fails `cargo test`. Not published.
//!
//! Each Markdown file is one module's docs. A block that is a sketch is fenced `rust,ignore` in the
//! document itself.

#[doc = include_str!("../../../CLA.md")]
pub mod cla {}

#[doc = include_str!("../../../CODE_OF_CONDUCT.md")]
pub mod code_of_conduct {}

#[doc = include_str!("../../../CONTRIBUTING.md")]
pub mod contributing {}

#[doc = include_str!("../../../README.md")]
pub mod readme {}

#[doc = include_str!("../../../SECURITY.md")]
pub mod security {}

#[doc = include_str!("../../../docs/architecture.md")]
pub mod docs_architecture {}

#[doc = include_str!("../../../docs/design/backends.md")]
pub mod docs_design_backends {}

#[doc = include_str!("../../../docs/design/border.md")]
pub mod docs_design_border {}

#[doc = include_str!("../../../docs/design/brush.md")]
pub mod docs_design_brush {}

#[doc = include_str!("../../../docs/design/builder-api.md")]
pub mod docs_design_builder_api {}

#[doc = include_str!("../../../docs/design/dsl-neutral.md")]
pub mod docs_design_dsl_neutral {}

#[doc = include_str!("../../../docs/design/element.md")]
pub mod docs_design_element {}

#[doc = include_str!("../../../docs/design/frame.md")]
pub mod docs_design_frame {}

#[doc = include_str!("../../../docs/design/golden-images.md")]
pub mod docs_design_golden_images {}

#[doc = include_str!("../../../docs/design/host.md")]
pub mod docs_design_host {}

#[doc = include_str!("../../../docs/design/layout-tests.md")]
pub mod docs_design_layout_tests {}

#[doc = include_str!("../../../docs/design/loader-values.md")]
pub mod docs_design_loader_values {}

#[doc = include_str!("../../../docs/design/naming-review.md")]
pub mod docs_design_naming_review {}

#[doc = include_str!("../../../docs/design/panels.md")]
pub mod docs_design_panels {}

#[doc = include_str!("../../../docs/design/shapes.md")]
pub mod docs_design_shapes {}

#[doc = include_str!("../../../docs/design/surface.md")]
pub mod docs_design_surface {}

#[doc = include_str!("../../../docs/design/text-block.md")]
pub mod docs_design_text_block {}

#[doc = include_str!("../../../docs/design/text.md")]
pub mod docs_design_text {}

#[doc = include_str!("../../../docs/design/ui-class.md")]
pub mod docs_design_ui_class {}

#[doc = include_str!("../../../docs/design/value-objects.md")]
pub mod docs_design_value_objects {}

#[doc = include_str!("../../../docs/design/view-pipeline.md")]
pub mod docs_design_view_pipeline {}

#[doc = include_str!("../../../docs/design/xaml-compiler.md")]
pub mod docs_design_xaml_compiler {}

#[doc = include_str!("../../../docs/spec/dsl.md")]
pub mod docs_spec_dsl {}

#[doc = include_str!("../../../docs/spec/technical_spec.md")]
pub mod docs_spec_technical_spec {}

#[doc = include_str!("../../../docs/spikes/linkme-registry.md")]
pub mod docs_spikes_linkme_registry {}
