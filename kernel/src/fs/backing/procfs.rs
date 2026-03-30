use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::fs::api::{DirEntry, FileType, FsError, Metadata};
use crate::fs::runtime::manager::FsStatFs;
use crate::fs::vfs::{FileSystem, Inode};
use crate::task::{self, ProcessId};

pub struct ProcfsFileSystem;

impl ProcfsFileSystem {
    pub const fn new() -> Self {
        Self
    }
}

impl FileSystem for ProcfsFileSystem {
    fn name(&self) -> &str {
        "procfs"
    }

    fn root(&self) -> Arc<dyn Inode> {
        Arc::new(ProcfsInode::root())
    }

    fn sync(&self) -> Result<(), FsError> {
        Ok(())
    }

    fn statfs(&self) -> Result<FsStatFs, FsError> {
        let process_count = task::runtime::manager::all_process_ids().len() as u64;
        Ok(FsStatFs {
            f_type: 0x0000_9fa0,
            f_bsize: 4096,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: process_count.saturating_add(4),
            f_ffree: 0,
            f_namelen: 255,
            f_frsize: 4096,
            f_flags: 1,
        })
    }
}

#[derive(Clone, Debug)]
enum ProcNode {
    Root,
    CpuInfo,
    MemInfo,
    ProcessDir { pid: usize },
    ProcessStatus { pid: usize },
    ProcessCmdline { pid: usize },
}

#[derive(Clone, Debug)]
struct ProcfsInode {
    node: ProcNode,
}

impl ProcfsInode {
    const fn root() -> Self {
        Self {
            node: ProcNode::Root,
        }
    }

    fn process_exists(pid: usize) -> bool {
        task::find_process_by_pid(ProcessId(pid)).is_some()
    }

    fn node_path(&self) -> String {
        match self.node {
            ProcNode::Root => String::from("/"),
            ProcNode::CpuInfo => String::from("/cpuinfo"),
            ProcNode::MemInfo => String::from("/meminfo"),
            ProcNode::ProcessDir { pid } => format!("/{pid}"),
            ProcNode::ProcessStatus { pid } => format!("/{pid}/status"),
            ProcNode::ProcessCmdline { pid } => format!("/{pid}/cmdline"),
        }
    }

    fn hash_path(path: &str) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in path.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0001_0000_01b3);
        }
        hash
    }

    fn as_directory(&self) -> bool {
        matches!(self.node, ProcNode::Root | ProcNode::ProcessDir { .. })
    }

    fn process_status(pid: usize) -> Result<String, FsError> {
        let process_ref = task::find_process_by_pid(ProcessId(pid)).ok_or(FsError::NotFound)?;
        let process = process_ref.lock();
        let (state_code, state_name) = match process.status {
            crate::task::proc::ProcessStatus::Running => ('R', "running"),
            crate::task::proc::ProcessStatus::Stopped => ('T', "stopped"),
            crate::task::proc::ProcessStatus::Exiting => ('X', "exiting"),
            crate::task::proc::ProcessStatus::Zombie => ('Z', "zombie"),
        };
        let ppid = process.parent.map(|parent| parent.0).unwrap_or(0);
        Ok(format!(
            "Name:\t{}\nState:\t{} ({})\nPid:\t{}\nPPid:\t{}\nTgid:\t{}\nThreads:\t{}\nVmPages:\t{}\nCmd:\t{}\n",
            process.name,
            state_code,
            state_name,
            process.pid.0,
            ppid,
            process.pid.0,
            process.task_count(),
            process.exec_mapping_count(),
            process.last_exec_path,
        ))
    }

    fn process_cmdline(pid: usize) -> Result<String, FsError> {
        let process_ref = task::find_process_by_pid(ProcessId(pid)).ok_or(FsError::NotFound)?;
        let process = process_ref.lock();
        let mut out = process.last_exec_path.clone();
        out.push('\0');
        Ok(out)
    }

    fn cpuinfo() -> String {
        let mut out = String::new();
        let cpu_count = crate::smp::cpu_count();
        for cpu in 0..cpu_count {
            out.push_str(
                format!(
                    "processor\t: {}\nvendor_id\t: january_os\nmodel name\t: january_os virtual cpu\ncpu cores\t: {}\n\n",
                    cpu, cpu_count
                )
                .as_str(),
            );
        }
        out
    }

    fn meminfo() -> String {
        let total_bytes = crate::mm::memblock_phys_mem_size();
        let reserved_bytes = crate::mm::memblock_reserved_size();
        let available_bytes = total_bytes.saturating_sub(reserved_bytes);
        let total_kib = total_bytes / 1024;
        let available_kib = available_bytes / 1024;
        format!(
            "MemTotal:\t{} kB\nMemFree:\t{} kB\nMemAvailable:\t{} kB\nMemReserved:\t{} kB\n",
            total_kib,
            available_kib,
            available_kib,
            reserved_bytes / 1024,
        )
    }

    fn render(&self) -> Result<Vec<u8>, FsError> {
        let text = match self.node {
            ProcNode::CpuInfo => Self::cpuinfo(),
            ProcNode::MemInfo => Self::meminfo(),
            ProcNode::ProcessStatus { pid } => Self::process_status(pid)?,
            ProcNode::ProcessCmdline { pid } => Self::process_cmdline(pid)?,
            ProcNode::Root | ProcNode::ProcessDir { .. } => return Err(FsError::IsDirectory),
        };
        Ok(text.into_bytes())
    }
}

impl Inode for ProcfsInode {
    fn metadata(&self) -> Result<Metadata, FsError> {
        match self.node {
            ProcNode::Root => Ok(Metadata {
                ino: Self::hash_path("/"),
                file_type: FileType::Directory,
                mode: 0o040555,
                size: 0,
                nlink: 2,
            }),
            ProcNode::CpuInfo
            | ProcNode::MemInfo
            | ProcNode::ProcessStatus { .. }
            | ProcNode::ProcessCmdline { .. } => {
                let data = self.render()?;
                Ok(Metadata {
                    ino: Self::hash_path(self.node_path().as_str()),
                    file_type: FileType::Regular,
                    mode: 0o100444,
                    size: data.len() as u64,
                    nlink: 1,
                })
            }
            ProcNode::ProcessDir { pid } => {
                if !Self::process_exists(pid) {
                    return Err(FsError::NotFound);
                }
                Ok(Metadata {
                    ino: Self::hash_path(self.node_path().as_str()),
                    file_type: FileType::Directory,
                    mode: 0o040555,
                    size: 0,
                    nlink: 2,
                })
            }
        }
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        if !self.as_directory() {
            return Err(FsError::NotDirectory);
        }

        if name.is_empty() || name == "." {
            return Ok(Arc::new(self.clone()));
        }

        match self.node {
            ProcNode::Root => match name {
                ".." => Ok(Arc::new(Self::root())),
                "cpuinfo" => Ok(Arc::new(Self {
                    node: ProcNode::CpuInfo,
                })),
                "meminfo" => Ok(Arc::new(Self {
                    node: ProcNode::MemInfo,
                })),
                _ => {
                    let pid = name.parse::<usize>().map_err(|_| FsError::NotFound)?;
                    if !Self::process_exists(pid) {
                        return Err(FsError::NotFound);
                    }
                    Ok(Arc::new(Self {
                        node: ProcNode::ProcessDir { pid },
                    }))
                }
            },
            ProcNode::ProcessDir { pid } => {
                if !Self::process_exists(pid) {
                    return Err(FsError::NotFound);
                }
                match name {
                    ".." => Ok(Arc::new(Self::root())),
                    "status" => Ok(Arc::new(Self {
                        node: ProcNode::ProcessStatus { pid },
                    })),
                    "cmdline" => Ok(Arc::new(Self {
                        node: ProcNode::ProcessCmdline { pid },
                    })),
                    _ => Err(FsError::NotFound),
                }
            }
            _ => Err(FsError::NotDirectory),
        }
    }

    fn readdir(&self) -> Result<Vec<DirEntry>, FsError> {
        match self.node {
            ProcNode::Root => {
                let mut out = Vec::from([
                    DirEntry {
                        ino: Self::hash_path("/cpuinfo"),
                        file_type: FileType::Regular,
                        name: String::from("cpuinfo"),
                    },
                    DirEntry {
                        ino: Self::hash_path("/meminfo"),
                        file_type: FileType::Regular,
                        name: String::from("meminfo"),
                    },
                ]);
                let mut pids = task::runtime::manager::all_process_ids();
                pids.sort_by_key(|pid| pid.0);
                for pid in pids {
                    out.push(DirEntry {
                        ino: Self::hash_path(format!("/{}", pid.0).as_str()),
                        file_type: FileType::Directory,
                        name: format!("{}", pid.0),
                    });
                }
                Ok(out)
            }
            ProcNode::ProcessDir { pid } => {
                if !Self::process_exists(pid) {
                    return Err(FsError::NotFound);
                }
                Ok(Vec::from([
                    DirEntry {
                        ino: Self::hash_path(format!("/{pid}/status").as_str()),
                        file_type: FileType::Regular,
                        name: String::from("status"),
                    },
                    DirEntry {
                        ino: Self::hash_path(format!("/{pid}/cmdline").as_str()),
                        file_type: FileType::Regular,
                        name: String::from("cmdline"),
                    },
                ]))
            }
            _ => Err(FsError::NotDirectory),
        }
    }

    fn read_at(&self, offset: usize, out: &mut [u8]) -> Result<usize, FsError> {
        let data = self.render()?;
        if offset >= data.len() || out.is_empty() {
            return Ok(0);
        }
        let n = out.len().min(data.len() - offset);
        out[..n].copy_from_slice(&data[offset..offset + n]);
        Ok(n)
    }
}
