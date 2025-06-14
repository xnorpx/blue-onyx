use std::{env, fs::File, path::Path, process::Command};
use zip::ZipArchive;

const ONNX_SOURCE: (&str, &str) = (
    "onnxruntime-1.22.0",
    "https://github.com/microsoft/onnxruntime/archive/refs/tags/v1.22.0.zip",
);

const DIRECTML_SOURCE: (&str, &str) = (
    "Microsoft.AI.DirectML.1.15.4",
    "https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/1.15.4",
);

macro_rules! build_error {
    ($($tokens: tt)*) => {
        println!("cargo::error={}", format!($($tokens)*))
    }
}

macro_rules! build_warning {
    ($($tokens: tt)*) => {
        println!("cargo::warning={}", format!($($tokens)*))
    }
}

fn get_build_config() -> &'static str {
    match env::var("PROFILE").as_deref() {
        Ok("release") => "Release",
        Ok("debug") => "Debug",
        _ => "Release",
    }
}

fn main() {
    build_warning!("Starting build script for ONNX Runtime");
    let target_dir = env::var("OUT_DIR").expect("OUT_DIR environment variable not set");

    check_and_download_onnx_source(&target_dir);
    if cfg!(windows) {
        check_and_download_directml(&target_dir);
    }

    let build_dir = Path::new(&target_dir).join(ONNX_SOURCE.0).join("build");

    let shared_lib_name = if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };

    let expected_binary = build_dir
        .join(if cfg!(windows) { "Windows" } else { "Linux" })
        .join(get_build_config())
        .join(if cfg!(windows) {
            get_build_config()
        } else {
            ""
        })
        .join(shared_lib_name);

    if build_dir.exists() && !expected_binary.exists() {
        build_warning!(
            "Build directory exists but expected binary missing, cleaning build directory"
        );
        std::fs::remove_dir_all(&build_dir).expect("Failed to clean build directory");
    }

    if !expected_binary.exists() {
        build_onnx(&target_dir);
    }

    if !expected_binary.exists() {
        build_error!("Expected ONNX Runtime binary not found after build");
        panic!("Build failed: ONNX Runtime binary missing");
    }

    let output_dir = Path::new(&target_dir)
        .ancestors()
        .nth(3)
        .expect("Failed to determine output directory");
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir).expect("Failed to create output directory");
    }

    std::fs::copy(&expected_binary, output_dir.join(shared_lib_name))
        .expect("Failed to copy ONNX Runtime binary to output directory");

    // On Windows, also copy DirectML.dll to the output directory if it does not exist
    if cfg!(windows) {
        let directml_dll = Path::new(&target_dir)
            .join(DIRECTML_SOURCE.0)
            .join("bin/x64-win/DirectML.dll");
        let output_dll = output_dir.join("DirectML.dll");
        if !output_dll.exists() {
            std::fs::copy(&directml_dll, &output_dll)
                .expect("Failed to copy DirectML.dll to output directory");
            build_warning!("Copied DirectML.dll to output directory");
        }
    }

    println!(
        "cargo:rustc-env=ORT_LIB_LOCATION={:?}",
        expected_binary.parent().unwrap()
    );
}

fn check_and_download_onnx_source(target_dir: &str) {
    let onnx_dir = Path::new(target_dir).join(ONNX_SOURCE.0);
    let zip_path = Path::new(target_dir).join("onnxruntime.zip");

    if !onnx_dir.exists() {
        if !zip_path.exists() {
            build_warning!("Downloading ONNX Runtime source");
            let mut response = reqwest::blocking::get(ONNX_SOURCE.1)
                .expect("Failed to download ONNX Runtime source");
            let mut file = File::create(&zip_path).expect("Failed to create ONNX Runtime zip file");
            response
                .copy_to(&mut file)
                .expect("Failed to write ONNX Runtime zip file");
        }

        build_warning!("Extracting ONNX Runtime source");
        let zip_file = File::open(&zip_path).expect("Failed to open ONNX Runtime zip file");
        let mut archive =
            ZipArchive::new(zip_file).expect("Failed to read ONNX Runtime zip archive");
        archive
            .extract(target_dir)
            .expect("Failed to extract ONNX Runtime source");
    }
}

fn check_and_download_directml(target_dir: &str) {
    let directml_dir = Path::new(target_dir).join(DIRECTML_SOURCE.0);
    let zip_path = Path::new(target_dir).join("directml.zip");
    let directml_for_build_dir = Path::new(target_dir).join("directml");

    if !directml_dir.exists() {
        if !zip_path.exists() {
            build_warning!("Downloading DirectML");
            let mut response =
                reqwest::blocking::get(DIRECTML_SOURCE.1).expect("Failed to download DirectML");
            let mut file = File::create(&zip_path).expect("Failed to create DirectML zip file");
            response
                .copy_to(&mut file)
                .expect("Failed to write DirectML zip file");
        }

        build_warning!("Extracting DirectML");
        let zip_file = File::open(&zip_path).expect("Failed to open DirectML zip file");
        let mut archive = ZipArchive::new(zip_file).expect("Failed to read DirectML zip archive");
        archive
            .extract(&directml_dir)
            .expect("Failed to extract DirectML");
    }

    let required_files = [
        directml_dir.join("bin/x64-win/DirectML.lib"),
        directml_dir.join("bin/x64-win/DirectML.dll"),
        directml_dir.join("include/DirectML.h"),
        directml_dir.join("include/DirectMLConfig.h"),
    ];

    for file in &required_files {
        if !file.exists() {
            build_error!("Required DirectML file missing: {:?}", file);
            panic!("DirectML setup incomplete");
        }
    }

    let directml_lib_dir = directml_dir.join("bin/x64-win");
    let directml_include_dir = directml_dir.join("include");
    let directml_lib_path = directml_lib_dir.join("DirectML.lib");
    let directml_dll_path = directml_lib_dir.join("DirectML.dll");
    let directml_include_path = directml_include_dir.join("DirectML.h");
    let directml_config_path = directml_include_dir.join("DirectMLConfig.h");

    let bin_dir = directml_for_build_dir.join("bin");
    let lib_dir = directml_for_build_dir.join("lib");
    let include_dir = directml_for_build_dir.join("include");

    std::fs::create_dir_all(&directml_for_build_dir)
        .expect("Failed to create direct ml for bin directory");
    std::fs::create_dir_all(&bin_dir).expect("Failed to create bin directory");
    std::fs::create_dir_all(&lib_dir).expect("Failed to create lib directory");
    std::fs::create_dir_all(&include_dir).expect("Failed to create include directory");

    std::fs::copy(&directml_lib_path, lib_dir.join("DirectML.lib"))
        .expect("Failed to copy DirectML.lib");
    std::fs::copy(&directml_dll_path, bin_dir.join("DirectML.dll"))
        .expect("Failed to copy DirectML.dll");
    std::fs::copy(&directml_include_path, include_dir.join("DirectML.h"))
        .expect("Failed to copy DirectML.h");
    std::fs::copy(&directml_config_path, include_dir.join("DirectMLConfig.h"))
        .expect("Failed to copy DirectMLConfig.h");

    // Verify files
    let copied_files = [
        lib_dir.join("DirectML.lib"),
        bin_dir.join("DirectML.dll"),
        include_dir.join("DirectML.h"),
        include_dir.join("DirectMLConfig.h"),
    ];

    for file in &copied_files {
        if !file.exists() {
            build_error!("Failed to verify copied file: {:?}", file);
            panic!("DirectML file copy verification failed");
        }
    }

    build_warning!("DirectML files copied and verified successfully");
}

fn build_onnx(target_dir: &str) {
    let onnx_dir = Path::new(target_dir).join(ONNX_SOURCE.0);
    let build_script = if cfg!(windows) {
        onnx_dir.join("build.bat")
    } else {
        onnx_dir.join("build.sh")
    };

    if !build_script.exists() {
        build_error!("Build script not found: {:?}", build_script);
        panic!("ONNX Runtime build script missing");
    }

    let mut build_commands = vec![
        "--config".to_string(),
        get_build_config().to_string(),
        "--build_shared_lib".to_string(),
        "--parallel".to_string(),
        num_cpus::get_physical().to_string(),
        "--compile_no_warning_as_error".to_string(),
        "--skip_tests".to_string(),
        "--enable_lto".to_string(),
        "--disable_contrib_ops".to_string(),
        "--cmake_extra_defines".to_string(),
        "onnxruntime_BUILD_UNIT_TESTS=OFF".to_string(),
    ];

    if cfg!(windows) {
        // Enable DirectML on Windows
        build_commands.extend([
            "--enable_msvc_static_runtime".to_string(),
            "--use_dml".to_string(),
            "--dml_path".to_string(),
            target_dir.to_string() + "\\directml",
        ]);
    } else if cfg!(target_os = "macos") {
        // Enable Core ML on macOS
        build_commands.push("--use_coreml".to_string());
    }

    build_warning!("Running ONNX Runtime build script");
    let status = Command::new(build_script)
        .args(&build_commands)
        .current_dir(&onnx_dir)
        .status()
        .expect("Failed to execute ONNX Runtime build script");

    if !status.success() {
        build_error!("ONNX Runtime build failed with status: {}", status);
        panic!("ONNX Runtime build failed");
    } else {
        build_warning!("ONNX Runtime build completed successfully");
    }
}
