// `PropertyInfo::access` and `AttachedPropertyInfo::access`: with the `access` feature, each
// derived property and attached property carries the thunks a runtime loader writes it through;
// without it, none carries any, so an app without a loader keeps none. CI runs this file both
// ways.

use artic_core::property::Property;
use artic_core::tree::NodeId;
use artic_macros::UiType;
use artic_registry::UiType;

#[derive(Debug, Clone, PartialEq, Default, UiType)]
#[ui(value)]
struct Needle {
  #[ui]
  length: f32,
}

#[derive(Debug, UiType)]
#[ui(attached(name = "Slot", ty = usize, target = any))]
struct Gauge {
  #[ui]
  width: Property<f32>,
  #[ui]
  label: Property<Option<String>>,
  #[ui(content)]
  child: Property<Option<NodeId>>,
}

// Without the feature, no property of a type or of a value type has thunks, nor does an attached
// property.
#[cfg(not(feature = "access"))]
#[test]
fn no_property_has_access_without_the_feature() {
  let properties = Gauge::TYPE_INFO.properties.iter().chain(Needle::TYPE_INFO.properties);

  for info in properties {
    assert!(info.access.is_none(), "{} has thunks", info.name);
  }
  for info in Gauge::TYPE_INFO.attached_properties {
    assert!(info.access.is_none(), "{} has thunks", info.name);
  }
}

// With the feature, every property of a type and of a value type has its thunks, and so does
// every attached property.
#[cfg(feature = "access")]
#[test]
fn every_property_has_access_with_the_feature() {
  let properties = Gauge::TYPE_INFO.properties.iter().chain(Needle::TYPE_INFO.properties);

  for info in properties {
    assert!(info.access.is_some(), "{} has no thunks", info.name);
  }
  for info in Gauge::TYPE_INFO.attached_properties {
    assert!(info.access.is_some(), "{} has no thunks", info.name);
  }
}
