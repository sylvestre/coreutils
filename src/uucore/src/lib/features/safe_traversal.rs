// Safe directory traversal using openat() and related syscalls
// This module provides TOCTOU-safe filesystem operations for recursive traversal
// Only available on Linux
// spell-checker:ignore CLOEXEC RDONLY TOCTOU closedir dirp fdopendir fstatat openat REMOVEDIR unlinkat

#![cfg(target_os = "linux")]

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::Path;

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
            unsafe { *libc::__errno_location() = 0 };

            let entry = unsafe { libc::readdir(dirp) };
            if entry.is_null() {
                let errno = unsafe { *libc::__errno_location() };
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

/// File information for tracking inodes
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FileInfo {
    pub dev: u64,
    pub ino: u64,
}

impl FileInfo {
    pub fn from_stat(stat: &libc::stat) -> Self {
        // Allow unnecessary cast because st_dev and st_ino have different types on different platforms
        #[allow(clippy::unnecessary_cast)]
        Self {
            dev: stat.st_dev as u64,
            ino: stat.st_ino as u64,
        }
    }
}
