/// Embed Windows icon/version resources only when building the desktop client.
fn main() {
    println!("cargo:rerun-if-changed=packaging/icons/VoxelCraft.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var_os("CARGO_FEATURE_CLIENT").is_some()
    {
        winresource::WindowsResource::new()
            .set_icon("packaging/icons/VoxelCraft.ico")
            .set("FileDescription", "VoxelCraft")
            .set("ProductName", "VoxelCraft")
            .set("OriginalFilename", "voxelcraft.exe")
            .compile()
            .expect("compile Windows icon and version resources");
    }
}
