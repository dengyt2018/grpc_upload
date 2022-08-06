use std::path::PathBuf;

#[cfg(windows)]
use winres::WindowsResource;

fn main() {
    let out_dir = PathBuf::from("src/libs");
    tonic_build::configure()
        .out_dir(out_dir)
        .compile(&["proto/upload.proto"], &["proto"])
        .unwrap();

    #[cfg(windows)]
    {
        WindowsResource::new()
            // This path can be absolute, or relative to your crate root.
            .set_icon("assets/icon.ico")
            .compile()
            .unwrap();
    }
}
