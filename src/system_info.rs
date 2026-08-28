use tracing::info;

pub fn system_info() -> anyhow::Result<()> {
    info!("System Information:");
    cpu_info()?;
    gpu_info(true)?;
    Ok(())
}

pub fn cpu_model() -> String {
    cpu_vendor_and_model().1
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn cpu_vendor_and_model() -> (String, String) {
    use raw_cpuid::CpuId;
    let cpuid = CpuId::new();
    let vendor = cpuid
        .get_vendor_info()
        .map_or_else(|| "Unknown".to_owned(), |info| info.as_str().to_owned());
    let model = cpuid
        .get_processor_brand_string()
        .map_or_else(|| "Unknown".to_owned(), |info| info.as_str().to_owned());
    (vendor, model)
}

#[cfg(all(
    target_os = "macos",
    not(any(target_arch = "x86", target_arch = "x86_64"))
))]
fn cpu_vendor_and_model() -> (String, String) {
    let model = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| "Apple Silicon".to_owned());
    ("Apple".to_owned(), model)
}

#[cfg(all(
    not(target_os = "macos"),
    not(any(target_arch = "x86", target_arch = "x86_64"))
))]
fn cpu_vendor_and_model() -> (String, String) {
    ("Unknown".to_owned(), std::env::consts::ARCH.to_owned())
}

pub fn gpu_model(index: usize) -> String {
    let gpu_names = gpu_info(false).unwrap_or_default();
    gpu_names
        .get(index)
        .cloned()
        .unwrap_or_else(|| "Unknown".to_owned())
}

pub fn cpu_info() -> anyhow::Result<()> {
    let (cpu_vendor_info, cpu_brand) = cpu_vendor_and_model();

    info!(
        "CPU | {} | {} | {} Cores | {} Logical Cores",
        cpu_vendor_info,
        cpu_brand,
        num_cpus::get_physical(),
        num_cpus::get()
    );
    Ok(())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn gpu_info(_log_info: bool) -> anyhow::Result<Vec<String>> {
    Ok(vec![]) // TODO: Do something for Linux
}

#[cfg(target_os = "macos")]
pub fn gpu_info(log_info: bool) -> anyhow::Result<Vec<String>> {
    let device_name = cpu_model();
    if log_info {
        info!("CoreML: {}", device_name);
    }
    Ok(vec![device_name])
}

#[cfg(windows)]
pub fn gpu_info(log_info: bool) -> anyhow::Result<Vec<String>> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_DESC1, IDXGIFactory1};
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1().map_err(|e| anyhow::anyhow!(e))? };
    let mut adapter_index = 0;
    let mut gpu_names = Vec::new();

    while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
        let desc: DXGI_ADAPTER_DESC1 =
            unsafe { adapter.GetDesc1().map_err(|e| anyhow::anyhow!(e))? };
        let device_name = String::from_utf16_lossy(&desc.Description);
        if !device_name.contains("Microsoft") {
            let mut device_name = String::from_utf16_lossy(&desc.Description);
            device_name = device_name.replace('\0', "");
            device_name = device_name.trim().to_string();
            device_name = device_name.split_whitespace().collect::<Vec<_>>().join(" ");
            if !gpu_names.contains(&device_name) {
                gpu_names.push(device_name.clone());
            }
        }
        adapter_index += 1;
    }

    gpu_names.sort();
    if log_info {
        for device_name in &gpu_names {
            info!("GPU: {}", device_name);
        }
    }

    Ok(gpu_names)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn print_cuda_gpu_info() {
        gpu_info(true).unwrap();
    }

    #[test]
    fn print_cpu_info() {
        cpu_info().unwrap()
    }
}
