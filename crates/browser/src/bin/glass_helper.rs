//! Glass Helper Process
//!
//! This is the helper executable for CEF subprocesses (GPU, Renderer, Plugin, etc.)
//! It must be bundled as separate .app bundles in Contents/Frameworks/

fn main() {
    #[cfg(target_os = "macos")]
    {
        use cef::library_loader::LibraryLoader;

        let exe_path = std::env::current_exe().expect("failed to get current exe path");

        // Try bundle-relative path first (helper is at
        // Glass Helper.app/Contents/MacOS/Glass Helper, framework is 3 levels up)
        let bundle_framework = exe_path
            .parent()
            .map(|p| {
                p.join("../../../Chromium Embedded Framework.framework/Chromium Embedded Framework")
            });

        let loaded = match bundle_framework {
            Some(ref path) if path.exists() => {
                let loader = LibraryLoader::new(&exe_path, true);
                if loader.load() {
                    std::mem::forget(loader);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        // Fall back to CEF_PATH env var or ~/.local/share/cef
        if !loaded {
            let cef_dir = match std::env::var("CEF_PATH") {
                Ok(path) => std::path::PathBuf::from(path),
                Err(_) => {
                    let home = std::env::var("HOME").expect("HOME not set");
                    std::path::PathBuf::from(home).join(".local/share/cef")
                }
            };

            let framework_path = cef_dir
                .join("Chromium Embedded Framework.framework/Chromium Embedded Framework");

            if !framework_path.exists() {
                eprintln!(
                    "CEF framework not found at bundle path or {}",
                    framework_path.display()
                );
                std::process::exit(1);
            }

            use std::os::unix::ffi::OsStrExt;
            let path_cstr = std::ffi::CString::new(framework_path.as_os_str().as_bytes())
                .expect("invalid CEF path");
            let result =
                unsafe { cef::load_library(Some(&*path_cstr.as_ptr().cast())) };
            if result != 1 {
                eprintln!("Failed to load CEF library from {}", framework_path.display());
                std::process::exit(1);
            }
        }

        // Initialize CEF API
        let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

        let args = cef::args::Args::new();

        // Execute the subprocess - this handles GPU, Renderer, etc.
        let exit_code = cef::execute_process(
            Some(args.as_main_args()),
            None::<&mut cef::App>,
            std::ptr::null_mut(),
        );

        // exit_code >= 0 means this was a subprocess, exit with that code
        if exit_code >= 0 {
            std::process::exit(exit_code);
        }

        // exit_code == -1 means this is the browser process (shouldn't happen for helper)
        eprintln!("Helper was invoked as browser process - this shouldn't happen");
        std::process::exit(1);
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("Helper is only needed on macOS");
        std::process::exit(1);
    }
}
