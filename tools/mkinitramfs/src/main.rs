use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const CPIO_NEWC_MAGIC: &str = "070701";
const TRAILER: &str = "TRAILER!!!";

struct MappingEntry {
    src: String,
    dest: String,
}

struct ArchiveNode {
    src: Option<PathBuf>,
    dest: String,
    mode: u32,
    is_dir: bool,
}

fn write_header(out: &mut Vec<u8>, ino: u32, mode: u32, file_size: u32, name_size: u32) {
    let mut header = String::with_capacity(110);
    header.push_str(CPIO_NEWC_MAGIC);
    header.push_str(&format!("{ino:08x}"));
    header.push_str(&format!("{mode:08x}"));
    header.push_str("00000000"); // uid
    header.push_str("00000000"); // gid
    header.push_str("00000001"); // nlink
    header.push_str("00000000"); // mtime
    header.push_str(&format!("{file_size:08x}"));
    header.push_str("00000000"); // devmajor
    header.push_str("00000000"); // devminor
    header.push_str("00000000"); // rdevmajor
    header.push_str("00000000"); // rdevminor
    header.push_str(&format!("{name_size:08x}"));
    header.push_str("00000000"); // check
    debug_assert_eq!(header.len(), 110);
    out.extend_from_slice(header.as_bytes());
}

fn write_entry(out: &mut Vec<u8>, ino: u32, mode: u32, dest: &str, data: &[u8]) {
    let name_size = dest.len() + 1;
    write_header(out, ino, mode, data.len() as u32, name_size as u32);

    out.extend_from_slice(dest.as_bytes());
    out.push(0);
    while out.len() % 4 != 0 {
        out.push(0);
    }

    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn normalize_dest(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        panic!("empty archive path");
    }
    if trimmed == "/" {
        panic!("archive path '/' is not allowed");
    }

    let mut out = String::new();
    for comp in trimmed.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            panic!("archive path must not contain '..': {trimmed}");
        }
        out.push('/');
        out.push_str(comp);
    }

    if out.is_empty() {
        panic!("archive path '{trimmed}' resolves to empty");
    }
    out
}

fn parse_mapping(arg: &str) -> MappingEntry {
    let Some((src, dest)) = arg.split_once(':') else {
        panic!("invalid mapping '{arg}', expected <src>:<dest>");
    };
    MappingEntry {
        src: src.to_string(),
        dest: normalize_dest(dest),
    }
}

#[cfg(unix)]
fn mode_from_metadata(metadata: &fs::Metadata, is_dir: bool) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    let perm = metadata.permissions().mode() & 0o777;
    if is_dir {
        0o040000 | perm.max(0o755)
    } else {
        0o100000 | perm.max(0o644)
    }
}

#[cfg(not(unix))]
fn mode_from_metadata(metadata: &fs::Metadata, is_dir: bool) -> u32 {
    if is_dir {
        0o040755
    } else if metadata.permissions().readonly() {
        0o100444
    } else {
        0o100644
    }
}

fn should_skip_entry(name: &str) -> bool {
    name == ".gitkeep" || name == ".keep"
}

fn walk_root(root: &Path, rel: &str, out: &mut Vec<ArchiveNode>) -> io::Result<()> {
    let mut children: Vec<_> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .collect();
    children.sort_by_key(|entry| entry.file_name());

    for child in children {
        let file_name = child.file_name();
        let name = file_name.to_string_lossy();
        if should_skip_entry(name.as_ref()) {
            continue;
        }

        let path = child.path();
        let metadata = fs::metadata(&path)?;
        let child_rel = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", rel, name)
        };

        if metadata.is_dir() {
            out.push(ArchiveNode {
                src: None,
                dest: normalize_dest(child_rel.as_str()),
                mode: mode_from_metadata(&metadata, true),
                is_dir: true,
            });
            walk_root(path.as_path(), child_rel.as_str(), out)?;
            continue;
        }

        if metadata.is_file() {
            out.push(ArchiveNode {
                src: Some(path),
                dest: normalize_dest(child_rel.as_str()),
                mode: mode_from_metadata(&metadata, false),
                is_dir: false,
            });
        }
    }

    Ok(())
}

fn collect_from_root(root: &Path) -> io::Result<Vec<ArchiveNode>> {
    if !root.exists() {
        panic!("initramfs root does not exist: {}", root.display());
    }
    if !root.is_dir() {
        panic!("initramfs root is not a directory: {}", root.display());
    }

    let mut nodes = Vec::new();
    walk_root(root, "", &mut nodes)?;
    nodes.sort_by(|a, b| a.dest.cmp(&b.dest));
    Ok(nodes)
}

fn collect_from_mappings(mappings: Vec<MappingEntry>) -> io::Result<Vec<ArchiveNode>> {
    let mut nodes = Vec::new();
    for mapping in mappings {
        let metadata = fs::metadata(&mapping.src)?;
        if !metadata.is_file() {
            panic!("mapping source is not a regular file: {}", mapping.src);
        }
        nodes.push(ArchiveNode {
            src: Some(PathBuf::from(&mapping.src)),
            dest: mapping.dest,
            mode: mode_from_metadata(&metadata, false),
            is_dir: false,
        });
    }
    nodes.sort_by(|a, b| a.dest.cmp(&b.dest));
    Ok(nodes)
}

fn main() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let output = args
        .next()
        .unwrap_or_else(|| panic!("usage: mkinitramfs <output.cpio> --root <dir> | <src:dest>..."));

    let nodes = match args.next() {
        Some(flag) if flag == "--root" => {
            let root = args
                .next()
                .unwrap_or_else(|| panic!("missing root directory after --root"));
            if args.next().is_some() {
                panic!("unexpected extra arguments after --root <dir>");
            }
            collect_from_root(Path::new(&root))?
        }
        Some(first_mapping) => {
            let mut mappings = vec![parse_mapping(first_mapping.as_str())];
            mappings.extend(args.map(|arg| parse_mapping(arg.as_str())));
            collect_from_mappings(mappings)?
        }
        None => panic!("missing archive input spec"),
    };

    let mut archive = Vec::new();
    let mut ino = 1u32;

    for node in nodes.iter().filter(|node| node.is_dir) {
        write_entry(
            &mut archive,
            ino,
            node.mode,
            node.dest.trim_start_matches('/'),
            &[],
        );
        ino = ino.saturating_add(1);
    }

    for node in nodes.iter().filter(|node| !node.is_dir) {
        let src = node.src.as_ref().expect("file node missing source");
        let data = fs::read(src)?;
        write_entry(
            &mut archive,
            ino,
            node.mode,
            node.dest.trim_start_matches('/'),
            &data,
        );
        ino = ino.saturating_add(1);
    }

    write_entry(&mut archive, ino, 0, TRAILER, &[]);

    let out_path = Path::new(&output);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::File::create(out_path)?;
    file.write_all(&archive)?;
    file.flush()?;

    Ok(())
}
