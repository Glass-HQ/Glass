//! Glass Helper Process
//!
//! This is the helper executable for CEF subprocesses (GPU, Renderer, Plugin, etc.)
//! It must be bundled as separate .app bundles in Contents/Frameworks/

fn main() {
    #[cfg(target_os = "macos")]
    {
        use cef::library_loader::LibraryLoader;

        // Load CEF library - helper uses relative path (true)
        let loader = LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        if !loader.load() {
            eprintln!("Failed to load CEF library");
            std::process::exit(1);
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
