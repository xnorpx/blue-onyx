use std::{
    env,
    fs::File,
    path::{Path, PathBuf},
    process::Command,
};
use zip::ZipArchive;

const ONNX_SOURCE: (&str, &str) = (
    "onnxruntime-1.29.0",
    "https://github.com/microsoft/onnxruntime/archive/refs/tags/v1.29.0.zip",
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
        .join(if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "MacOS"
        } else {
            "Linux"
        })
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

    println!("cargo:rustc-env=ORT_DYLIB_PATH={shared_lib_name}");
}

fn check_and_download_onnx_source(target_dir: &str) {
    let onnx_dir = Path::new(target_dir).join(ONNX_SOURCE.0);
    let zip_path = Path::new(target_dir).join(format!("{}.zip", ONNX_SOURCE.0));
    let extraction_dir = Path::new(target_dir).join(format!("{}.extracting", ONNX_SOURCE.0));

    let build_script = if cfg!(windows) {
        onnx_dir.join("build.bat")
    } else {
        onnx_dir.join("build.sh")
    };
    if onnx_dir.exists()
        && (!build_script.exists() || !onnx_dir.join("tools/ci_build/build.py").exists())
    {
        build_warning!("ONNX Runtime source is incomplete, removing it");
        std::fs::remove_dir_all(&onnx_dir).expect("Failed to remove incomplete ONNX source");
    }

    if !onnx_dir.exists() {
        ensure_archive("ONNX Runtime source", ONNX_SOURCE.1, &zip_path);
        build_warning!("Extracting ONNX Runtime source");
        extract_archive("ONNX Runtime source", &zip_path, &extraction_dir);
        std::fs::rename(extraction_dir.join(ONNX_SOURCE.0), &onnx_dir)
            .expect("Failed to move extracted ONNX Runtime source into place");
        std::fs::remove_dir_all(&extraction_dir)
            .expect("Failed to remove ONNX Runtime extraction directory");
    }

    disable_unused_pytorch_probe(&onnx_dir);
}

fn disable_unused_pytorch_probe(onnx_dir: &Path) {
    const PYTORCH_PROBE: &str = r#"have_torch = importlib.util.find_spec("torch")
if have_torch:
    from .pytorch_export_helpers import infer_input_info  # noqa: F401"#;

    for relative_path in [
        "tools/python/util/__init__.py",
        "tools/python/util/__init__append.py",
    ] {
        let file_path = onnx_dir.join(relative_path);
        let contents = std::fs::read_to_string(&file_path)
            .unwrap_or_else(|error| panic!("Failed to read {}: {error}", file_path.display()));

        if contents.contains(PYTORCH_PROBE) {
            std::fs::write(
                &file_path,
                contents.replace(PYTORCH_PROBE, "have_torch = None"),
            )
            .unwrap_or_else(|error| panic!("Failed to patch {}: {error}", file_path.display()));
        } else if !contents.contains("have_torch = None") {
            panic!(
                "ONNX Runtime's PyTorch probe changed; refusing to run an environment-dependent build"
            );
        }
    }
}

fn check_and_download_directml(target_dir: &str) {
    let directml_dir = Path::new(target_dir).join(DIRECTML_SOURCE.0);
    let zip_path = Path::new(target_dir).join(format!("{}.zip", DIRECTML_SOURCE.0));
    let extraction_dir = Path::new(target_dir).join(format!("{}.extracting", DIRECTML_SOURCE.0));
    let directml_for_build_dir = Path::new(target_dir).join("directml");
    let required_files = directml_required_files(&directml_dir);

    if directml_dir.exists() && required_files.iter().any(|file| !file.exists()) {
        build_warning!("DirectML source is incomplete, removing it");
        std::fs::remove_dir_all(&directml_dir)
            .expect("Failed to remove incomplete DirectML source");
    }

    if !directml_dir.exists() {
        ensure_archive("DirectML", DIRECTML_SOURCE.1, &zip_path);
        build_warning!("Extracting DirectML");
        extract_archive("DirectML", &zip_path, &extraction_dir);
        std::fs::rename(&extraction_dir, &directml_dir)
            .expect("Failed to move extracted DirectML source into place");
    }

    for file in directml_required_files(&directml_dir) {
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

fn directml_required_files(directml_dir: &Path) -> [PathBuf; 4] {
    [
        directml_dir.join("bin/x64-win/DirectML.lib"),
        directml_dir.join("bin/x64-win/DirectML.dll"),
        directml_dir.join("include/DirectML.h"),
        directml_dir.join("include/DirectMLConfig.h"),
    ]
}

fn ensure_archive(name: &str, url: &str, archive_path: &Path) {
    if archive_path.exists() && !archive_is_valid(archive_path) {
        build_warning!("Cached {} archive is invalid, downloading it again", name);
        std::fs::remove_file(archive_path).expect("Failed to remove invalid cached archive");
    }

    if archive_path.exists() {
        return;
    }

    build_warning!("Downloading {}", name);
    let partial_path = archive_path.with_extension("part");
    if partial_path.exists() {
        std::fs::remove_file(&partial_path).expect("Failed to remove partial archive");
    }

    let mut response = reqwest::blocking::get(url)
        .and_then(reqwest::blocking::Response::error_for_status)
        .unwrap_or_else(|error| panic!("Failed to download {name}: {error}"));
    let mut file = File::create(&partial_path).expect("Failed to create partial archive");
    response
        .copy_to(&mut file)
        .unwrap_or_else(|error| panic!("Failed to write {name} archive: {error}"));
    file.sync_all().expect("Failed to sync downloaded archive");
    drop(file);

    if !archive_is_valid(&partial_path) {
        std::fs::remove_file(&partial_path).expect("Failed to remove invalid downloaded archive");
        panic!("Downloaded {name} archive is invalid");
    }

    std::fs::rename(partial_path, archive_path)
        .expect("Failed to move downloaded archive into place");
}

fn archive_is_valid(archive_path: &Path) -> bool {
    File::open(archive_path)
        .ok()
        .and_then(|file| ZipArchive::new(file).ok())
        .is_some()
}

fn extract_archive(name: &str, archive_path: &Path, extraction_dir: &Path) {
    if extraction_dir.exists() {
        std::fs::remove_dir_all(extraction_dir)
            .unwrap_or_else(|error| panic!("Failed to clean {name} extraction directory: {error}"));
    }
    std::fs::create_dir_all(extraction_dir)
        .unwrap_or_else(|error| panic!("Failed to create {name} extraction directory: {error}"));

    let zip_file = File::open(archive_path)
        .unwrap_or_else(|error| panic!("Failed to open {name} archive: {error}"));
    let mut archive = ZipArchive::new(zip_file)
        .unwrap_or_else(|error| panic!("Failed to read {name} archive: {error}"));
    archive
        .extract(extraction_dir)
        .unwrap_or_else(|error| panic!("Failed to extract {name}: {error}"));
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
        "--no_telemetry".to_string(),
        "--enable_lto".to_string(),
        "--disable_contrib_ops".to_string(),
        "--cmake_extra_defines".to_string(),
        "onnxruntime_BUILD_UNIT_TESTS=OFF".to_string(),
        "CMAKE_POLICY_VERSION_MINIMUM=3.5".to_string(),
        "FETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER".to_string(),
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
