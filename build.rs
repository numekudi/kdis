use std::env;

fn main() {
    println!("cargo:rerun-if-changed=resources/windows/kdis.rc");
    println!("cargo:rerun-if-changed=resources/windows/kdis.manifest.xml");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Cross builds do not run GPUI's host-gated Windows resource step, so the
        // application embeds its own Common Controls v6 manifest for TaskDialogIndirect.
        embed_resource::compile("resources/windows/kdis.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed the Windows application manifest");
    }
}
