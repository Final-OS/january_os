//! VFS core tests.

use crate::fs::vfs;
use crate::{error, kprintln, ok};

use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    kprintln!("=== VFS Core Tests ===");
    if crate::config::DEBUG_VERBOSE {
        kprintln!("[test/vfs] filter={:?}", filter);
    }

    match filter {
        None | Some("all") => {
            test_path();
            test_mount();
        }
        Some("path") => test_path(),
        Some("mount") => test_mount(),
        Some(name) => {
            error!("Unknown VFS test: {}", name);
            kprintln!("Available vfs tests: path, mount");
        }
    }

    kprintln!();
}

fn test_path() {
    match test_path_case() {
        Ok(()) => pass("path"),
        Err(msg) => fail("path", msg.as_str()),
    }
}

fn test_mount() {
    match test_mount_case() {
        Ok(()) => pass("mount"),
        Err(msg) => fail("mount", msg.as_str()),
    }
}

fn test_path_case() -> Result<(), alloc::string::String> {
    use alloc::string::String;

    let p = vfs::normalize_path("/a/b", "../c").map_err(|e| format!("{:?}", e))?;
    if p != "/a/c" {
        return Err(format!("normalize ../ failed: got {}", p));
    }

    let p = vfs::normalize_path("/", "./x//y/").map_err(|e| format!("{:?}", e))?;
    if p != "/x/y" {
        return Err(format!("normalize dot/slash failed: got {}", p));
    }

    let (parent, name) = vfs::split_parent("/a/b/c");
    if parent != "/a/b" || name != "c" {
        return Err(String::from("split_parent mismatch"));
    }

    Ok(())
}

#[derive(Clone)]
struct MockInode;

impl vfs::Inode for MockInode {
    fn metadata(&self) -> Result<vfs::Metadata, vfs::FsError> {
        Ok(vfs::Metadata::empty())
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn vfs::Inode>, vfs::FsError> {
        Err(vfs::FsError::NotFound)
    }

    fn readdir(&self) -> Result<Vec<vfs::DirEntry>, vfs::FsError> {
        Ok(Vec::new())
    }

    fn read_at(&self, _offset: usize, _out: &mut [u8]) -> Result<usize, vfs::FsError> {
        Ok(0)
    }
}

#[derive(Clone)]
struct MockFs {
    name: &'static str,
}

impl vfs::FileSystem for MockFs {
    fn name(&self) -> &str {
        self.name
    }

    fn root(&self) -> Arc<dyn vfs::Inode> {
        Arc::new(MockInode)
    }

    fn sync(&self) -> Result<(), vfs::FsError> {
        Ok(())
    }
}

static MOUNT_SEQ: AtomicUsize = AtomicUsize::new(1);

fn test_mount_case() -> Result<(), alloc::string::String> {
    use alloc::format;
    use alloc::string::String;

    let seq = MOUNT_SEQ.fetch_add(1, Ordering::Relaxed);
    let target = format!("/mnt_vfs_{}", seq);
    let result = (|| {
        let root = Arc::new(MockFs { name: "mock-root" });
        vfs::mount_root(root);

        let sub = Arc::new(MockFs { name: "mock-sub" });
        vfs::mount_fs(target.as_str(), sub).map_err(|e| format!("{:?}", e))?;

        let resolved = vfs::resolve_mount("/some/path")
            .ok_or_else(|| String::from("resolve_mount root returned none"))?;
        if resolved.0 != "/" {
            return Err(format!("expected root mount, got {}", resolved.0));
        }

        let sub_path = format!("{}/a/b", target);
        let resolved = vfs::resolve_mount(sub_path.as_str())
            .ok_or_else(|| String::from("resolve_mount sub returned none"))?;
        if resolved.0 != target {
            return Err(format!("expected {} mount, got {}", target, resolved.0));
        }

        vfs::umount_fs(target.as_str()).map_err(|e| format!("{:?}", e))?;
        Ok(())
    })();

    // Keep VFS tests isolated: always restore runtime default root mount.
    vfs::mount_root(Arc::new(vfs::staticfs::StaticFileSystem::new()));
    result
}

fn pass(name: &str) {
    ok!("vfs/{}", name);
}

fn fail(name: &str, reason: &str) {
    error!("vfs/{}: {}", name, reason);
}
