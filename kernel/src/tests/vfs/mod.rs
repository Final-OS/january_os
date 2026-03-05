//! VFS core tests.

use crate::fs;
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
            test_fd_bridge();
        }
        Some("path") => test_path(),
        Some("mount") => test_mount(),
        Some("fd_bridge") => test_fd_bridge(),
        Some(name) => {
            error!("Unknown VFS test: {}", name);
            kprintln!("Available vfs tests: path, mount, fd_bridge");
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

fn test_fd_bridge() {
    match test_fd_bridge_case() {
        Ok(()) => pass("fd_bridge"),
        Err(msg) => fail("fd_bridge", msg.as_str()),
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
struct MockBridgeRootInode;

#[derive(Clone)]
struct MockBridgeFileInode;

const MOCK_BRIDGE_FILE: &str = "hello.txt";
const MOCK_BRIDGE_DATA: &[u8] = b"vfs-bridge";

impl vfs::Inode for MockBridgeRootInode {
    fn metadata(&self) -> Result<vfs::Metadata, vfs::FsError> {
        Ok(vfs::Metadata {
            ino: 1,
            file_type: vfs::FileType::Directory,
            mode: 0o040755,
            size: 0,
            nlink: 2,
        })
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn vfs::Inode>, vfs::FsError> {
        if name == MOCK_BRIDGE_FILE {
            return Ok(Arc::new(MockBridgeFileInode));
        }
        Err(vfs::FsError::NotFound)
    }

    fn readdir(&self) -> Result<Vec<vfs::DirEntry>, vfs::FsError> {
        Ok(Vec::from([vfs::DirEntry {
            ino: 2,
            file_type: vfs::FileType::Regular,
            name: alloc::string::String::from(MOCK_BRIDGE_FILE),
        }]))
    }

    fn read_at(&self, _offset: usize, _out: &mut [u8]) -> Result<usize, vfs::FsError> {
        Err(vfs::FsError::IsDirectory)
    }
}

impl vfs::Inode for MockBridgeFileInode {
    fn metadata(&self) -> Result<vfs::Metadata, vfs::FsError> {
        Ok(vfs::Metadata {
            ino: 2,
            file_type: vfs::FileType::Regular,
            mode: 0o100644,
            size: MOCK_BRIDGE_DATA.len() as u64,
            nlink: 1,
        })
    }

    fn lookup(&self, _name: &str) -> Result<Arc<dyn vfs::Inode>, vfs::FsError> {
        Err(vfs::FsError::NotDirectory)
    }

    fn readdir(&self) -> Result<Vec<vfs::DirEntry>, vfs::FsError> {
        Err(vfs::FsError::NotDirectory)
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, vfs::FsError> {
        if offset >= MOCK_BRIDGE_DATA.len() || out.is_empty() {
            return Ok(0);
        }
        let n = out.len().min(MOCK_BRIDGE_DATA.len() - offset);
        out[..n].copy_from_slice(&MOCK_BRIDGE_DATA[offset..offset + n]);
        Ok(n)
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

#[derive(Clone)]
struct MockBridgeFs;

impl vfs::FileSystem for MockBridgeFs {
    fn name(&self) -> &str {
        "mock-bridge"
    }

    fn root(&self) -> Arc<dyn vfs::Inode> {
        Arc::new(MockBridgeRootInode)
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

fn test_fd_bridge_case() -> Result<(), alloc::string::String> {
    use alloc::format;

    let seq = MOUNT_SEQ.fetch_add(1, Ordering::Relaxed);
    let target = format!("/mnt_vfs_fd_{}", seq);
    vfs::mount_fs(target.as_str(), Arc::new(MockBridgeFs)).map_err(|e| format!("{:?}", e))?;

    let pid = 0xfaceusize;
    let file_path = format!("{}/{}", target, MOCK_BRIDGE_FILE);
    let fd = fs::open_for_pid(pid, file_path.as_str(), 0, 0)
        .map_err(|errno| format!("open errno={}", errno))?;

    let mut buf = [0u8; 32];
    let n = fs::read_for_pid(pid, fd, &mut buf).map_err(|errno| format!("read errno={}", errno))?;
    if &buf[..n] != MOCK_BRIDGE_DATA {
        let _ = fs::close_for_pid(pid, fd);
        fs::drop_process_fds(pid);
        let _ = vfs::umount_fs(target.as_str());
        return Err(format!("content mismatch: got={:?}", &buf[..n]));
    }
    let _ = fs::close_for_pid(pid, fd);

    fs::chdir_for_pid(pid, target.as_str()).map_err(|errno| format!("chdir errno={}", errno))?;
    let rel_fd = fs::open_for_pid(pid, MOCK_BRIDGE_FILE, 0, 0)
        .map_err(|errno| format!("relative open errno={}", errno))?;
    let mut rel = [0u8; 32];
    let rel_n = fs::read_for_pid(pid, rel_fd, &mut rel)
        .map_err(|errno| format!("relative read errno={}", errno))?;
    if &rel[..rel_n] != MOCK_BRIDGE_DATA {
        let _ = fs::close_for_pid(pid, rel_fd);
        fs::drop_process_fds(pid);
        let _ = vfs::umount_fs(target.as_str());
        return Err(format!(
            "relative content mismatch: got={:?}",
            &rel[..rel_n]
        ));
    }

    let _ = fs::close_for_pid(pid, rel_fd);
    fs::drop_process_fds(pid);
    vfs::umount_fs(target.as_str()).map_err(|e| format!("{:?}", e))?;
    Ok(())
}

fn pass(name: &str) {
    ok!("vfs/{}", name);
}

fn fail(name: &str, reason: &str) {
    error!("vfs/{}: {}", name, reason);
}
