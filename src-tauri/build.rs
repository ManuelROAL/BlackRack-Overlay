use std::{env, fs, path::PathBuf};

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

fn repair_tauri_static_vcruntime_placeholder() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
        || env::var("STATIC_VCRUNTIME").as_deref() != Ok("true")
    {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR no está definido"));
    let library_path = out_dir.join("msvcrt.lib");
    let Ok(contents) = fs::read(&library_path) else {
        return;
    };

    // Tauri 2.6.x writes a COFF object with a .lib extension to shadow the
    // dynamic CRT. MSVC 14.44 rejects that file as an invalid library. Wrap the
    // same placeholder object in a real archive so the static CRT link flags
    // emitted by tauri-build continue to work as intended.
    if contents.starts_with(b"!<arch>\n") {
        return;
    }

    let object_path = out_dir.join("tauri-msvcrt-placeholder.obj");
    let repaired_path = out_dir.join("tauri-msvcrt-placeholder.lib");
    fs::write(&object_path, contents).expect("no se pudo escribir el placeholder de msvcrt");
    let _ = fs::remove_file(&repaired_path);

    let target = env::var("TARGET").expect("TARGET no está definido");
    let mut librarian = cc::windows_registry::find(&target, "lib.exe")
        .expect("no se encontró lib.exe para reparar el runtime estático de Tauri");
    let status = librarian
        .arg("/NOLOGO")
        .arg(format!("/OUT:{}", repaired_path.display()))
        .arg(&object_path)
        .status()
        .expect("no se pudo ejecutar lib.exe para reparar el runtime estático de Tauri");
    assert!(
        status.success(),
        "lib.exe no pudo reparar el runtime estático de Tauri"
    );

    fs::remove_file(&library_path).expect("no se pudo reemplazar el placeholder de msvcrt");
    fs::rename(&repaired_path, &library_path)
        .expect("no se pudo instalar la biblioteca msvcrt reparada");
    let _ = fs::remove_file(object_path);
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

    tauri_build::build();
    repair_tauri_static_vcruntime_placeholder();
}
