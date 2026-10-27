use vello::wgpu::{Backend, Backends, DeviceType};

/// The GPU API to use on `os`, as [`std::env::consts::OS`] names it: DX12 on Windows, Metal on
/// macOS and iOS, Vulkan elsewhere.
pub(crate) fn backends_for(os: &str) -> Backends {
  match os {
    "windows" => Backends::DX12,
    "macos" | "ios" => Backends::METAL,
    _ => Backends::VULKAN,
  }
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

  // WARP is a CPU adapter on DX12; lavapipe, a CPU adapter on Vulkan, and real GPUs are usable.
  #[test]
  fn only_dx12_cpu_adapters_are_warp() {
    assert!(is_warp(Backend::Dx12, DeviceType::Cpu));
    assert!(!is_warp(Backend::Vulkan, DeviceType::Cpu));
    assert!(!is_warp(Backend::Dx12, DeviceType::DiscreteGpu));
  }
}
