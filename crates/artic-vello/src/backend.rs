use vello::wgpu::{Backend, Backends, DeviceType, PowerPreference};

/// The GPU API to use on `os`, as [`std::env::consts::OS`] names it: DX12 on Windows, Metal on
/// macOS and iOS, Vulkan elsewhere.
pub(crate) fn backends_for(os: &str) -> Backends {
  match os {
    "windows" => Backends::DX12,
    "macos" | "ios" => Backends::METAL,
    _ => Backends::VULKAN,
  }
}

/// The GPU to ask for: the one `WGPU_POWER_PREF` names, read by wgpu into `from_env`, or else the
/// low-power one, which is plenty for a 2D UI and saves battery.
pub(crate) fn power_for(from_env: Option<PowerPreference>) -> PowerPreference {
  from_env.unwrap_or(PowerPreference::LowPower)
}

/// The variable that names the adapter to use, as in other wgpu apps.
pub(crate) const ADAPTER_NAME_VARIABLE: &str = "WGPU_ADAPTER_NAME";

/// The adapter name `value`, the variable's value, asks for: none when it's unset or blank, since a
/// blank name would match every adapter and skip the power preference.
pub(crate) fn adapter_name(value: Option<String>) -> Option<String> {
  value.filter(|name| !name.trim().is_empty())
}

/// The first of `names` that contains `wanted`, ignoring case, as wgpu matches `WGPU_ADAPTER_NAME`.
pub(crate) fn named<'a>(wanted: &str, names: impl IntoIterator<Item = &'a str>) -> Option<usize> {
  let wanted = wanted.to_lowercase();
  names.into_iter().position(|name| name.to_lowercase().contains(&wanted))
}

/// Whether an adapter on `backend` of type `device` is WARP, Windows' software DX12 adapter, on
/// which vello's compute shaders crash. lavapipe, a CPU adapter on Vulkan, isn't.
pub(crate) fn is_warp(backend: Backend, device: DeviceType) -> bool {
  backend == Backend::Dx12 && device == DeviceType::Cpu
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // Each desktop OS gets its native GPU API, whichever OS runs the test.
  #[test]
  fn backend_for_target() {
    let chosen: Vec<Backends> = ["windows", "macos", "linux"].into_iter().map(backends_for).collect();

    assert_eq!(chosen, [Backends::DX12, Backends::METAL, Backends::VULKAN]);
  }

  // Without `WGPU_POWER_PREF` the low-power GPU is asked for; a set preference wins.
  #[test]
  fn power_defaults_to_low() {
    assert_eq!(power_for(None), PowerPreference::LowPower);
    assert_eq!(
      power_for(Some(PowerPreference::HighPerformance)),
      PowerPreference::HighPerformance
    );
    assert_eq!(power_for(Some(PowerPreference::None)), PowerPreference::None);
  }

  // `WGPU_ADAPTER_NAME` picks the first adapter whose name contains it, ignoring case.
  #[test]
  fn adapter_is_named_by_part_of_its_name() {
    let names = ["NVIDIA GeForce RTX 3050", "Intel(R) UHD Graphics", "Intel(R) Arc"];

    assert_eq!(named("intel", names), Some(1));
    assert_eq!(named("GEFORCE", names), Some(0));
    assert_eq!(named("radeon", names), None);
  }

  // A blank `WGPU_ADAPTER_NAME` names no adapter, so the power preference decides, as when unset.
  #[test]
  fn blank_adapter_name_is_unset() {
    assert_eq!(adapter_name(None), None);
    assert_eq!(adapter_name(Some(String::new())), None);
    assert_eq!(adapter_name(Some("  ".to_owned())), None);
    assert_eq!(adapter_name(Some("intel".to_owned())), Some("intel".to_owned()));
  }

  // WARP is a CPU adapter on DX12; lavapipe, a CPU adapter on Vulkan, and real GPUs are usable.
  #[test]
  fn only_dx12_cpu_adapters_are_warp() {
    assert!(is_warp(Backend::Dx12, DeviceType::Cpu));
    assert!(!is_warp(Backend::Vulkan, DeviceType::Cpu));
    assert!(!is_warp(Backend::Dx12, DeviceType::DiscreteGpu));
  }
}
