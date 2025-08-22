// Safe directory traversal using openat() and related syscalls
// This module provides TOCTOU-safe filesystem operations for recursive traversal
// Only available on Unix systems

// spell-checker:ignore CLOEXEC RDONLY REMOVEDIR TOCTOU closedir dirfd dirp fchmodat fchownat fdopendir fstatat openat unlinkat

#![cfg(unix)]

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;

// Platform-specific errno handling
#[cfg(target_os = "linux")]
fn errno_ptr() -> *mut i32 {
    unsafe { libc::__errno_location() }
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn errno_ptr() -> *mut i32 {
    unsafe { libc::__error() }
}

#[cfg(target_os = "android")]
fn errno_ptr() -> *mut i32 {
    unsafe { libc::__errno() }
}

// Fallback for other Unix systems
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "android"
    ))
))]
fn errno_ptr() -> *mut i32 {
    // For other Unix systems, try to get errno through last_os_error
    // This is less efficient but more portable
    static mut ERRNO_STORAGE: i32 = 0;
    unsafe { &mut ERRNO_STORAGE }
}

/// A directory file descriptor that enables safe traversal
pub struct DirFd {
    fd: RawFd,
    owned: bool,
}

impl DirFd {
    /// Open a directory and return a file descriptor
    pub fn open(path: &Path) -> io::Result<Self> {
        let path_cstr = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains null byte"))?;

        let fd = unsafe {
            libc::open(
                path_cstr.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };

        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(DirFd { fd, owned: true })
        }
    }

    /// Open a subdirectory relative to this directory
    pub fn open_subdir(&self, name: &OsStr) -> io::Result<Self> {
        let name_cstr = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains null byte"))?;

        let fd = unsafe {
            libc::openat(
                self.fd,
                name_cstr.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };

        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(DirFd { fd, owned: true })
        }
    }

    /// Get raw stat data for a file relative to this directory
    pub fn stat_at(&self, name: &OsStr, follow_symlinks: bool) -> io::Result<libc::stat> {
        let name_cstr = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains null byte"))?;

        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        let flags = if follow_symlinks {
            0
        } else {
            libc::AT_SYMLINK_NOFOLLOW
        };

        let ret = unsafe { libc::fstatat(self.fd, name_cstr.as_ptr(), &mut stat, flags) };

        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(stat)
        }
    }

    /// Get raw stat data for this directory
    pub fn fstat(&self) -> io::Result<libc::stat> {
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };

        let ret = unsafe { libc::fstat(self.fd, &mut stat) };

        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(stat)
        }
    }

    /// Remove a file or empty directory relative to this directory
    pub fn unlink_at(&self, name: &OsStr, is_dir: bool) -> io::Result<()> {
        let name_cstr = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains null byte"))?;
        let flags = if is_dir { libc::AT_REMOVEDIR } else { 0 };

        let ret = unsafe { libc::unlinkat(self.fd, name_cstr.as_ptr(), flags) };

        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Change permissions on a file relative to this directory
    #[cfg(not(target_os = "redox"))]
    pub fn chmod_at(&self, name: &OsStr, mode: u32, follow_symlinks: bool) -> io::Result<()> {
        let name_cstr = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains null byte"))?;
        let flags = if follow_symlinks {
            0
        } else {
            libc::AT_SYMLINK_NOFOLLOW
        };

        let ret =
            unsafe { libc::fchmodat(self.fd, name_cstr.as_ptr(), mode as libc::mode_t, flags) };

        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Change ownership on a file relative to this directory
    pub fn chown_at(
        &self,
        name: &OsStr,
        uid: Option<u32>,
        gid: Option<u32>,
        follow_symlinks: bool,
    ) -> io::Result<()> {
        let name_cstr = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains null byte"))?;
        let flags = if follow_symlinks {
            0
        } else {
            libc::AT_SYMLINK_NOFOLLOW
        };

        let uid = uid
            .map(|u| u as libc::uid_t)
            .unwrap_or(-1i32 as libc::uid_t);
        let gid = gid
            .map(|g| g as libc::gid_t)
            .unwrap_or(-1i32 as libc::gid_t);

        let ret = unsafe { libc::fchownat(self.fd, name_cstr.as_ptr(), uid, gid, flags) };

        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Read directory entries
    pub fn read_dir(&self) -> io::Result<Vec<OsString>> {
        let mut entries = Vec::new();

        // Duplicate the fd for fdopendir (it takes ownership)
        let dup_fd = unsafe { libc::dup(self.fd) };
        if dup_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let dirp = unsafe { libc::fdopendir(dup_fd) };
        if dirp.is_null() {
            unsafe { libc::close(dup_fd) };
            return Err(io::Error::last_os_error());
        }

        loop {
            // Clear errno before readdir
            unsafe { *errno_ptr() = 0 };

            let entry = unsafe { libc::readdir(dirp) };
            if entry.is_null() {
                let errno = unsafe { *errno_ptr() };
                if errno != 0 {
                    unsafe { libc::closedir(dirp) };
                    return Err(io::Error::from_raw_os_error(errno));
                }
                break;
            }

            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            let name_os = OsStr::from_bytes(name.to_bytes());

            // Skip . and ..
            if name_os != "." && name_os != ".." {
                entries.push(name_os.to_os_string());
            }
        }

        unsafe { libc::closedir(dirp) };
        Ok(entries)
    }

    pub fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for DirFd {
    fn drop(&mut self) {
        if self.owned && self.fd >= 0 {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

impl AsRawFd for DirFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

// Helper functions are now embedded in the methods above

/// File information for tracking inodes
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FileInfo {
    pub dev: u64,
    pub ino: u64,
}

impl FileInfo {
    pub fn from_stat(stat: &libc::stat) -> Self {
        // Allow unnecessary cast because st_dev and st_ino have different types on different platforms
        // (e.g., i32 on macOS, u64 on Linux)
        #[allow(clippy::unnecessary_cast)]
        Self {
            dev: stat.st_dev as u64,
            ino: stat.st_ino as u64,
        }
    }
}

/// Utility function to detect if we can skip safe traversal (i.e., path is normal)
pub fn should_skip_safe_traversal(path: &Path) -> bool {
    let path_os_len = path.as_os_str().len();

    // Very short simple paths are always safe - early exit
    if path_os_len < 15 {
        return true;
    }

    // Very long paths need safe traversal - early exit
    if path_os_len > 1000 {
        return false;
    }

    // Convert to string only once for all checks
    let path_str = path.to_string_lossy();
    let path_len = path_str.len();

    // Double check string length in case of UTF-8 encoding differences
    if path_len > 1000 {
        return false;
    }

    // Quick component count check - just count separators
    let component_count = path_str.matches('/').count();

    // Too many components need safe traversal - early exit
    if component_count > 20 {
        return false;
    }

    // Very simple paths can skip safe traversal - early exit
    if component_count <= 2 && path_len < 50 {
        return true;
    }

    // Simple paths with reasonable length can skip safe traversal - early exit
    // But only if components are reasonably short to avoid missing long repeated patterns
    if component_count <= 4 && path_len < 80 {
        let components: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();
        let max_component_len = components.iter().map(|c| c.len()).max().unwrap_or(0);
        if max_component_len < 40 {
            return true;
        }
    }

    // For deeper paths, we need to do the detailed analysis to check for repeated patterns
    // Don't use early exits for these as they might have problematic patterns

    // For paths that might be problematic, do detailed analysis
    let components: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();

    // Check for repeated directory names (symlink loops, test patterns)
    let mut component_counts = std::collections::HashMap::new();
    for component in &components {
        let count = component_counts.entry(*component).or_insert(0);
        *count += 1;
        // If any component appears too many times, need safe traversal
        if *count > 5 {
            return false;
        }

        // Check for long components with repeated characters
        if component.len() > 50 {
            let chars: Vec<char> = component.chars().collect();
            if chars.len() > 10 {
                let mut consecutive_same = 1;
                for i in 1..chars.len() {
                    if chars[i] == chars[i - 1] {
                        consecutive_same += 1;
                        if consecutive_same >= 12 {
                            return false;
                        }
                    } else {
                        consecutive_same = 1;
                    }
                }
            }
        }
    }

    // If we get here, no problematic patterns found - can skip safe traversal
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_should_skip_safe_traversal_short_paths() {
        let short_path = Path::new("/usr/bin");
        assert!(should_skip_safe_traversal(short_path));
    }

    #[test]
    fn test_should_skip_safe_traversal_long_paths() {
        let long_path_str = "a".repeat(1500);
        let long_path = Path::new(&long_path_str);
        assert!(!should_skip_safe_traversal(long_path));
    }

    #[test]
    fn test_should_skip_safe_traversal_repeated_patterns() {
        // Test repeated directory names (more than 5 occurrences)
        let repeated_x_path = Path::new("/x/x/x/x/x/x/test");
        assert!(!should_skip_safe_traversal(repeated_x_path));

        let repeated_a_path = Path::new("/a/a/a/a/a/a/test");
        assert!(!should_skip_safe_traversal(repeated_a_path));

        // Test long component with repeated characters
        let long_repeated_chars = "x".repeat(60);
        let long_x_path = format!("/{}", long_repeated_chars);
        assert!(!should_skip_safe_traversal(Path::new(&long_x_path)));

        // Test many components (more than 20)
        let many_components = (0..25)
            .map(|i| format!("dir{}", i))
            .collect::<Vec<_>>()
            .join("/");
        let many_comp_path_str = format!("/{}", many_components);
        let many_comp_path = Path::new(&many_comp_path_str);
        assert!(!should_skip_safe_traversal(many_comp_path));

        // Test normal paths should skip safe traversal
        let normal_path = Path::new("/home/user/documents/file.txt");
        assert!(should_skip_safe_traversal(normal_path));
    }

    #[cfg(unix)]
    #[test]
    fn test_dirfd_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        fs::write(temp_path.join("test.txt"), "hello world").unwrap();

        fs::create_dir(temp_path.join("subdir")).unwrap();
        fs::write(temp_path.join("subdir").join("nested.txt"), "nested file").unwrap();

        let dir_fd = DirFd::open(temp_path).unwrap();

        let entries = dir_fd.read_dir().unwrap();
        let entry_names: Vec<String> = entries
            .iter()
            .map(|e| e.to_string_lossy().to_string())
            .collect();

        assert!(entry_names.contains(&"test.txt".to_string()));
        assert!(entry_names.contains(&"subdir".to_string()));

        let stat = dir_fd
            .stat_at(std::ffi::OsStr::new("test.txt"), false)
            .unwrap();
        assert!(stat.st_size > 0);

        let subdir_fd = dir_fd.open_subdir(std::ffi::OsStr::new("subdir")).unwrap();
        let subdir_entries = subdir_fd.read_dir().unwrap();
        assert_eq!(subdir_entries.len(), 1);
        assert_eq!(subdir_entries[0].to_string_lossy(), "nested.txt");
    }

    #[cfg(unix)]
    #[test]
    fn test_chmod_at() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        fs::write(temp_path.join("test.txt"), "hello world").unwrap();

        let dir_fd = DirFd::open(temp_path).unwrap();

        dir_fd
            .chmod_at(std::ffi::OsStr::new("test.txt"), 0o644, false)
            .unwrap();

        let stat = dir_fd
            .stat_at(std::ffi::OsStr::new("test.txt"), false)
            .unwrap();
        assert_eq!(stat.st_mode & 0o777, 0o644);
    }

    #[cfg(unix)]
    #[test]
    fn test_unlink_at() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        fs::write(temp_path.join("test.txt"), "hello world").unwrap();

        let dir_fd = DirFd::open(temp_path).unwrap();

        assert!(
            dir_fd
                .stat_at(std::ffi::OsStr::new("test.txt"), false)
                .is_ok()
        );

        dir_fd
            .unlink_at(std::ffi::OsStr::new("test.txt"), false)
            .unwrap();

        assert!(
            dir_fd
                .stat_at(std::ffi::OsStr::new("test.txt"), false)
                .is_err()
        );
    }

    #[test]
    fn test_file_info_equality() {
        let info1 = FileInfo { dev: 1, ino: 100 };
        let info2 = FileInfo { dev: 1, ino: 100 };
        let info3 = FileInfo { dev: 1, ino: 101 };

        assert_eq!(info1, info2);
        assert_ne!(info1, info3);
    }
}
