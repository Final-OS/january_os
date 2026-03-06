use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::api::FsError;

pub fn normalize_path(cwd: &str, input: &str) -> Result<String, FsError> {
    if input.is_empty() {
        return Err(FsError::NotFound);
    }

    let mut parts: Vec<&str> = Vec::new();
    if !input.starts_with('/') {
        for comp in cwd.split('/') {
            if comp.is_empty() || comp == "." {
                continue;
            }
            if comp == ".." {
                let _ = parts.pop();
            } else {
                parts.push(comp);
            }
        }
    }

    for comp in input.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp == ".." {
            let _ = parts.pop();
        } else {
            parts.push(comp);
        }
    }

    if parts.is_empty() {
        return Ok(String::from("/"));
    }

    let mut out = String::from("/");
    for (idx, comp) in parts.iter().enumerate() {
        if idx != 0 {
            out.push('/');
        }
        out.push_str(comp);
    }
    Ok(out)
}

pub fn split_parent(path: &str) -> (&str, &str) {
    if path == "/" {
        return ("/", "");
    }
    let trimmed = path.trim_end_matches('/');
    if let Some(idx) = trimmed.rfind('/') {
        if idx == 0 {
            return ("/", &trimmed[1..]);
        }
        (&trimmed[..idx], &trimmed[idx + 1..])
    } else {
        ("/", trimmed)
    }
}
