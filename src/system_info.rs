use tracing::info;

pub fn system_info() -> anyhow::Result<()> {
    info!("System Information:");
    cpu_info()?;
    gpu_info(true)?;
    Ok(())
}

pub fn cpu_model() -> String {
    use raw_cpuid::CpuId;
    let cpuid = CpuId::new();
    match cpuid.get_processor_brand_string() {
        Some(cpu_brand) => cpu_brand.as_str().to_owned(),
        None => "Unknown".to_owned(),
    }
}

pub fn gpu_model(index: usize) -> String {
    let gpu_names = gpu_info(false).unwrap_or_default();
    gpu_names
        .get(index)
        .cloned()
        .unwrap_or_else(|| "Unknown".to_owned())
}

pub fn cpu_info() -> anyhow::Result<()> {
    use raw_cpuid::CpuId;
    let cpuid = CpuId::new();

    let cpu_vendor_info = match cpuid.get_vendor_info() {
        Some(vendor_info) => vendor_info.as_str().to_owned(),
        None => "Unknown".to_owned(),
    };

    let cpu_brand = match cpuid.get_processor_brand_string() {
        Some(cpu_brand) => cpu_brand.as_str().to_owned(),
        None => "Unknown".to_owned(),
    };

    info!(
        "CPU | {} | {} | {} Cores | {} Logical Cores",
        cpu_vendor_info,
        cpu_brand,
        num_cpus::get_physical(),
        num_cpus::get()
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn gpu_info(_log_info: bool) -> anyhow::Result<Vec<String>> {
    Ok(vec![]) // TODO: Do something for Linux
}

#[cfg(windows)]
pub fn gpu_info(log_info: bool) -> anyhow::Result<Vec<String>> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_DESC1};
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
    let mut adapter_index = 0;
    let mut gpu_names = Vec::new();

    while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
        let desc: DXGI_ADAPTER_DESC1 = unsafe { adapter.GetDesc1()? };
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
        for (index, device_name) in gpu_names.iter().enumerate() {
            info!("GPU {} | {}", index, device_name);
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
