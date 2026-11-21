fn main() {
    tauri_build::build();

    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let target = std::env::var("TARGET").unwrap();

    if target.contains("android") {
        println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
    }

    if target.contains("linux") && !target.contains("android") {
        let lib_dir = std::path::Path::new(&manifest)
            .join("..")
            .join("desktop")
            .join("libzt")
            .join("linux-x64");
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=zt");
        for lib in ["stdc++", "pthread", "dl", "m", "rt"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }

    if target.contains("windows") {
        let lib_dir = std::path::Path::new(&manifest)
            .join("..")
            .join("desktop")
            .join("libzt")
            .join("windows-x64");
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=zt");
        for lib in [
            "ws2_32",
            "shlwapi",
            "iphlpapi",
            "bcrypt",
            "advapi32",
            "userenv",
            "ntdll",
            "ole32",
            "shell32",
            "user32",
            "winmm",
            "libcpmt",
        ] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }
}
