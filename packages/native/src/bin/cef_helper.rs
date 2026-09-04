use cef::{api_hash, api_version, args::Args, execute_process, library_loader::LibraryLoader};

fn main() {
    let args = Args::new();
    let sandbox_enabled = !std::env::args_os().any(|argument| argument == "--no-sandbox");
    let _sandbox = if sandbox_enabled {
        let mut sandbox = cef::sandbox::Sandbox::new();
        sandbox.initialize(args.as_main_args());
        Some(sandbox)
    } else {
        None
    };

    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Chromium helper executable path is unavailable: {error}");
            std::process::exit(1);
        }
    };
    let framework = executable
        .parent()
        .map(|directory| {
            directory
                .join("../../..")
                .join("Chromium Embedded Framework.framework/Chromium Embedded Framework")
        })
        .and_then(|path| path.canonicalize().ok());
    if framework.is_none() {
        eprintln!(
            "Chromium Embedded Framework is missing beside helper {}",
            executable.display()
        );
        std::process::exit(1);
    }
    let loader = LibraryLoader::new(&executable, true);
    if !loader.load() {
        eprintln!("Could not load Chromium Embedded Framework");
        std::process::exit(1);
    }
    let expected_api = cef::sys::CEF_API_VERSION_LAST;
    if api_hash(expected_api, 0).is_null() || api_version() != expected_api {
        eprintln!("Chromium helper CEF API is incompatible with version {expected_api}");
        std::process::exit(1);
    }

    let exit_code = execute_process(
        Some(args.as_main_args()),
        None::<&mut cef::App>,
        std::ptr::null_mut(),
    );
    std::process::exit(exit_code.max(0));
}
