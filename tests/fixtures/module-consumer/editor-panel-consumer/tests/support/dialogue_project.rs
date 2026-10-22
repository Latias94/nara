use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct DialogueProject {
    pub root: PathBuf,
}

impl DialogueProject {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "nara-dialogue-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generic-editor/known-unbound"),
            &root,
        );
        fs::create_dir(root.join("dialogue")).unwrap();
        fs::write(
            root.join("dialogue/intro.json"),
            br#"["Welcome","Goodbye"]"#,
        )
        .unwrap();
        Self { root }
    }

    #[allow(dead_code)] // The panel consumer opens by path; the Host consumer needs a capability.
    pub fn capability(&self) -> nara::fs::DirectoryCapability {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(&self.root).unwrap();
        nara::fs::DirectoryCapability::from_host_handle(
            handle,
            nara::fs::HostCapabilityOptions::new(
                nara::fs::CapabilityRights::ReadWrite,
                nara::fs::TrustMode::TrustedLocal,
            ),
        )
        .unwrap()
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

impl Drop for DialogueProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
