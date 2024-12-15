use std::env;
use std::{fs::File, io::Write, path::Path, process::Command};
use zip::ZipArchive;

const ONNX_SOURCE: (&str, &str) = (
    "onnxruntime-1.20.1",
    "https://github.com/microsoft/onnxruntime/archive/refs/tags/v1.20.1.zip",
);

const DIRECTML: &str = "https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/1.15.4";

const ONNX_BUILD_COMMANDS: [&str; 11] = [
    "--build_shared_lib",
    "--parallel",
    "--compile_no_warning_as_error",
    "--skip_tests",
    "--disable_exceptions",
    "--disable_rtti",
    "--enable_msvc_static_runtime",
    "--enable_lto",
    "--disable_contrib_ops",
    "--cmake_extra_defines",
    "onnxruntime_BUILD_UNIT_TESTS=OFF",
];

fn get_build_config() -> &'static str {
    match env::var("PROFILE").as_deref() {
        Ok("release") => "Release",
        Ok("debug") => "Debug",
        _ => "Release",
    }
}

fn main() {
    let target_dir = env::var("OUT_DIR").unwrap();
    check_and_download_onnx_source(&target_dir);
    check_and_download_directml(&target_dir);

    let build_dir = std::path::Path::new(&target_dir)
        .join(ONNX_SOURCE.0)
        .join("build");
    if !build_dir.exists() {
        build_onnx(&target_dir);
    }

    let onnx_dir = std::path::Path::new(&target_dir)
        .join(ONNX_SOURCE.0)
        .join("build")
        .join("Windows")
        .join(get_build_config());
    println!("cargo:rustc-env=ORT_LIB_LOCATION={:?}", onnx_dir);
}

fn check_and_download_onnx_source(target_dir: &str) {
    let onnx_dir = std::path::Path::new(target_dir).join(ONNX_SOURCE.0);
    let zip_path = std::path::Path::new(target_dir).join("onnxruntime.zip");

    if !onnx_dir.exists() {
        if !zip_path.exists() {
            let response = ureq::get(ONNX_SOURCE.1).call().unwrap();
            let mut file = File::create(&zip_path).unwrap();
            let mut reader = response.into_reader();
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer).unwrap();
            file.write_all(&buffer).unwrap();
        }

        let zip_file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(zip_file).unwrap();
        archive.extract(target_dir).unwrap();
    }
}

fn check_and_download_directml(target_dir: &str) {
    let directml_dir = std::path::Path::new(target_dir).join("Microsoft.AI.DirectML.1.15.4");
    let zip_path = std::path::Path::new(target_dir).join("directml.zip");

    if !directml_dir.exists() {
        if !zip_path.exists() {
            let response = ureq::get(DIRECTML).call().unwrap();
            let mut file = File::create(&zip_path).unwrap();
            let mut reader = response.into_reader();
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer).unwrap();
            file.write_all(&buffer).unwrap();
        }

        let zip_file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(zip_file).unwrap();
        archive.extract(directml_dir.clone()).unwrap();
    }

    let directml_lib = directml_dir
        .join("bin")
        .join("x64-win")
        .join("DirectML.lib");
    let directml_dll = directml_dir
        .join("bin")
        .join("x64-win")
        .join("DirectML.dll");
    let directml_header = directml_dir.join("include").join("DirectML.h");
    let directml_header_config = directml_dir.join("include").join("DirectMLConfig.h");

    let directml_library_dir = std::path::Path::new(target_dir).join("directml");
    if !directml_library_dir.exists() {
        std::fs::create_dir(&directml_library_dir).unwrap();
    }
    let directml_library_bin_dir = std::path::Path::new(&directml_library_dir).join("bin");
    if !directml_library_bin_dir.exists() {
        std::fs::create_dir(&directml_library_bin_dir).unwrap();
    }
    std::fs::copy(&directml_dll, directml_library_bin_dir.join("DirectML.dll")).unwrap();

    let output_dir = Path::new(target_dir).ancestors().nth(3).unwrap();
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).unwrap();
    }
    std::fs::copy(&directml_dll, output_dir.join("DirectML.dll")).unwrap();
    let directml_library_lib_dir = std::path::Path::new(&directml_library_dir).join("lib");
    if !directml_library_lib_dir.exists() {
        std::fs::create_dir(&directml_library_lib_dir).unwrap();
    }
    std::fs::copy(&directml_lib, directml_library_lib_dir.join("DirectML.lib")).unwrap();
    let directml_library_include_dir = std::path::Path::new(&directml_library_dir).join("include");
    if !directml_library_include_dir.exists() {
        std::fs::create_dir(&directml_library_include_dir).unwrap();
    }
    std::fs::copy(
        &directml_header,
        directml_library_include_dir.join("DirectML.h"),
    )
    .unwrap();
    std::fs::copy(
        &directml_header_config,
        directml_library_include_dir.join("DirectMLConfig.h"),
    )
    .unwrap();
}

fn build_onnx(target_dir: &str) {
    let onnx_dir = std::path::Path::new(target_dir).join(ONNX_SOURCE.0);
    let build_script = onnx_dir.join("build.bat");
    let directml_dir = std::path::Path::new(target_dir).join("directml");

    let mut build_commands = vec![
        "--config".to_string(),
        get_build_config().to_string(),
        "--use_dml".to_string(),
        "--dml_path".to_string(),
        directml_dir.to_str().unwrap().to_string(),
    ];
    build_commands.extend(ONNX_BUILD_COMMANDS.iter().map(|&s| s.to_string()));

    let status = Command::new(build_script)
        .args(&build_commands)
        .current_dir(&onnx_dir)
        .status()
        .expect("Failed to execute build script");

    if !status.success() {
        println!("cargo:warning=Build script failed with status: {}", status);
    } else {
        println!("cargo:warning=Build script executed successfully");
    }

    let onnx_runtime_dll = std::path::Path::new(&target_dir)
        .join(ONNX_SOURCE.0)
        .join("build")
        .join("Windows")
        .join(get_build_config())
        .join(get_build_config())
        .join("onnxruntime.dll");

    let output_dir = Path::new(target_dir).ancestors().nth(3).unwrap();
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).unwrap();
    }
    std::fs::copy(&onnx_runtime_dll, output_dir.join("onnxruntime.dll")).unwrap();
}
