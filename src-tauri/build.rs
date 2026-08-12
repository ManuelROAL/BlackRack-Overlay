use std::{env, path::PathBuf};

fn find_lmu_sdk() -> Option<PathBuf> {
    if let Some(path) = env::var_os("LMU_SHARED_MEMORY_SDK") {
        let candidate = PathBuf::from(path);
        if candidate.join("SharedMemoryInterface.hpp").is_file() {
            return Some(candidate);
        }
    }

    let relative = PathBuf::from(
        "SteamLibrary/steamapps/common/Le Mans Ultimate/Support/SharedMemoryInterface",
    );
    for drive in b'C'..=b'Z' {
        let candidate = PathBuf::from(format!("{}:/", drive as char)).join(&relative);
        if candidate.join("SharedMemoryInterface.hpp").is_file() {
            return Some(candidate);
        }
    }

    if let Some(program_files) = env::var_os("ProgramFiles(x86)") {
        let candidate = PathBuf::from(program_files)
            .join("Steam/steamapps/common/Le Mans Ultimate/Support/SharedMemoryInterface");
        if candidate.join("SharedMemoryInterface.hpp").is_file() {
            return Some(candidate);
        }
    }

    None
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(lmu_sdk)");
    println!("cargo:rerun-if-env-changed=LMU_SHARED_MEMORY_SDK");
    println!("cargo:rerun-if-changed=src/telemetry/lmu_bridge.cpp");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if let Some(sdk) = find_lmu_sdk() {
            println!(
                "cargo:warning=Compilando con el SDK de LMU en {}",
                sdk.display()
            );
            println!("cargo:rustc-cfg=lmu_sdk");
            if let Some(install_directory) = sdk.parent().and_then(|support| support.parent()) {
                println!(
                    "cargo:rustc-env=LMU_INSTALL_DIR={}",
                    install_directory.display()
                );
            }
            cc::Build::new()
                .cpp(true)
                .file("src/telemetry/lmu_bridge.cpp")
                .include(sdk)
                .flag_if_supported("/std:c++17")
                .compile("lmu_bridge");
        } else {
            println!(
                "cargo:warning=No se encontró el SDK de LMU; se compilará con telemetría simulada"
            );
        }
    }

    tauri_build::build()
}
