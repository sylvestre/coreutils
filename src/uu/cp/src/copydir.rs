// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore TODO canonicalizes direntry pathbuf symlinked IRWXO IRWXG
//! Recursively copy the contents of a directory.
//!
//! See the [`copy_directory`] function for more information.
#[cfg(windows)]
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf, StripPrefixError};

use indicatif::ProgressBar;
use uucore::display::Quotable;
use uucore::error::UIoError;
use uucore::fs::{
    FileInformation, MissingHandling, ResolveMode, canonicalize, path_ends_with_terminator,
};
use uucore::show;
#[cfg(windows)]
use uucore::show_error;
use uucore::translate;
use uucore::uio_error;

#[cfg(unix)]
use uucore::safe_traversal::DirFd;
#[cfg(windows)]
use walkdir::{DirEntry, WalkDir};

use crate::{CopyResult, Options, copy_file};
use crate::{CpError, aligned_ancestors, context_for, copy_attributes, copy_link};

/// Ensure a Windows path starts with a `\\?`.
#[cfg(target_os = "windows")]
fn adjust_canonicalization(p: &Path) -> Cow<'_, Path> {
    // In some cases, \\? can be missing on some Windows paths.  Add it at the
    // beginning unless the path is prefixed with a device namespace.
    const VERBATIM_PREFIX: &str = r"\\?";
    const DEVICE_NS_PREFIX: &str = r"\\.";

    let has_prefix = p
        .components()
        .next()
        .and_then(|comp| comp.as_os_str().to_str())
        .is_some_and(|p_str| {
            p_str.starts_with(VERBATIM_PREFIX) || p_str.starts_with(DEVICE_NS_PREFIX)
        });

    if has_prefix {
        p.into()
    } else {
        Path::new(VERBATIM_PREFIX).join(p).into()
    }
}

/// Get a descendant path relative to the given parent directory.
///
/// If `root_parent` is `None`, then this just returns the `path`
/// itself. Otherwise, this function strips the parent prefix from the
/// given `path`, leaving only the portion of the path relative to the
/// parent.
fn get_local_to_root_parent(
    path: &Path,
    root_parent: Option<&Path>,
) -> Result<PathBuf, StripPrefixError> {
    match root_parent {
        Some(parent) => {
            // On Windows, some paths are starting with \\?
            // but not always, so, make sure that we are consistent for strip_prefix
            // See https://docs.microsoft.com/en-us/windows/win32/fileio/naming-a-file for more info
            #[cfg(windows)]
            let (path, parent) = (
                adjust_canonicalization(path),
                adjust_canonicalization(parent),
            );
            let path = path.strip_prefix(parent)?;
            Ok(path.to_path_buf())
        }
        None => Ok(path.to_path_buf()),
    }
}

/// Paths that are invariant throughout the traversal when copying a directory.
struct Context<'a> {
    /// The current working directory at the time of starting the traversal.
    current_dir: PathBuf,

    /// The path to the parent of the source directory, if any.
    root_parent: Option<PathBuf>,

    /// The target path to which the directory will be copied.
    target: &'a Path,

    /// The source path from which the directory will be copied.
    root: &'a Path,
}

impl<'a> Context<'a> {
    fn new(root: &'a Path, target: &'a Path) -> io::Result<Self> {
        let current_dir = env::current_dir()?;
        let root_path = current_dir.join(root);
        let root_parent = if target.exists() && !root.to_str().unwrap().ends_with("/.") {
            root_path.parent().map(|p| p.to_path_buf())
        } else {
            Some(root_path)
        };
        Ok(Self {
            current_dir,
            root_parent,
            target,
            root,
        })
    }
}

/// Data needed to perform a single copy operation while traversing a directory.
///
/// For convenience while traversing a directory, the [`Entry::new`]
/// function allows creating an entry from a [`Context`] and a
/// [`walkdir::DirEntry`].
///
/// # Examples
///
/// For example, if the source directory structure is `a/b/c`, the
/// target is `d/`, a directory that already exists, and the copy
/// command is `cp -r a/b/c d`, then the overall set of copy
/// operations could be represented as three entries,
///
/// ```rust,ignore
/// let operations = [
///     Entry {
///         source_absolute: "/tmp/a".into(),
///         source_relative: "a".into(),
///         local_to_target: "d/a".into(),
///         target_is_file: false,
///     }
///     Entry {
///         source_absolute: "/tmp/a/b".into(),
///         source_relative: "a/b".into(),
///         local_to_target: "d/a/b".into(),
///         target_is_file: false,
///     }
///     Entry {
///         source_absolute: "/tmp/a/b/c".into(),
///         source_relative: "a/b/c".into(),
///         local_to_target: "d/a/b/c".into(),
///         target_is_file: false,
///     }
/// ];
/// ```
#[derive(Clone)]
struct Entry {
    /// The absolute path to file or directory to copy.
    source_absolute: PathBuf,

    /// The relative path to file or directory to copy.
    source_relative: PathBuf,

    /// The path to the destination, relative to the target.
    local_to_target: PathBuf,

    /// Whether the destination is a file.
    target_is_file: bool,
}

impl Entry {
    fn new<A: AsRef<Path>>(
        context: &Context,
        source: A,
        no_target_dir: bool,
    ) -> Result<Self, StripPrefixError> {
        let source = source.as_ref();
        let source_relative = source.to_path_buf();
        let source_absolute = context.current_dir.join(&source_relative);
        let mut descendant =
            get_local_to_root_parent(&source_absolute, context.root_parent.as_deref())?;
        if no_target_dir {
            let source_is_dir = source.is_dir();
            if path_ends_with_terminator(context.target) && source_is_dir {
                if let Err(e) = fs::create_dir_all(context.target) {
                    eprintln!(
                        "{}",
                        translate!("cp-error-failed-to-create-directory", "error" => e)
                    );
                }
            } else {
                descendant = descendant.strip_prefix(context.root)?.to_path_buf();
            }
        }

        let local_to_target = context.target.join(descendant);
        let target_is_file = context.target.is_file();
        Ok(Self {
            source_absolute,
            source_relative,
            local_to_target,
            target_is_file,
        })
    }

    #[cfg(windows)]
    fn from_walkdir(
        context: &Context,
        direntry: &DirEntry,
        no_target_dir: bool,
    ) -> Result<Self, StripPrefixError> {
        Self::new(context, direntry.path(), no_target_dir)
    }
}

#[allow(clippy::too_many_arguments)]
/// Copy a single entry during a directory traversal.
fn copy_direntry(
    progress_bar: Option<&ProgressBar>,
    entry: Entry,
    options: &Options,
    symlinked_files: &mut HashSet<FileInformation>,
    preserve_hard_links: bool,
    copied_destinations: &HashSet<PathBuf>,
    copied_files: &mut HashMap<FileInformation, PathBuf>,
) -> CopyResult<()> {
    let Entry {
        source_absolute,
        source_relative,
        local_to_target,
        target_is_file,
    } = entry;

    // If the source is a symbolic link and the options tell us not to
    // dereference the link, then copy the link object itself.
    if source_absolute.is_symlink() && !options.dereference {
        return copy_link(&source_absolute, &local_to_target, symlinked_files, options);
    }

    // If the source is a directory and the destination does not
    // exist, ...
    if source_absolute.is_dir() && !local_to_target.exists() {
        return if target_is_file {
            Err(translate!("cp-error-cannot-overwrite-non-directory-with-directory").into())
        } else {
            build_dir(&local_to_target, false, options, Some(&source_absolute))?;
            if options.verbose {
                println!("{}", context_for(&source_relative, &local_to_target));
            }
            Ok(())
        };
    }

    // If the source is not a directory, then we need to copy the file.
    if !source_absolute.is_dir() {
        if let Err(err) = copy_file(
            progress_bar,
            &source_absolute,
            local_to_target.as_path(),
            options,
            symlinked_files,
            copied_destinations,
            copied_files,
            false,
        ) {
            if preserve_hard_links {
                if !source_absolute.is_symlink() {
                    return Err(err);
                }
                // silent the error with a symlink
                // In case we do --archive, we might copy the symlink
                // before the file itself
            } else {
                // At this point, `path` is just a plain old file.
                // Terminate this function immediately if there is any
                // kind of error *except* a "permission denied" error.
                //
                // TODO What other kinds of errors, if any, should
                // cause us to continue walking the directory?
                match err {
                    CpError::IoErrContext(e, _) if e.kind() == io::ErrorKind::PermissionDenied => {
                        show!(uio_error!(
                            e,
                            "{}",
                            translate!("cp-error-cannot-open-for-reading", "source" => source_relative.quote()),
                        ));
                    }
                    e => return Err(e),
                }
            }
        }
    }

    // In any other case, there is nothing to do, so we just return to
    // continue the traversal.
    Ok(())
}

/// Read the contents of the directory `root` and recursively copy the
/// contents to `target`.
///
/// Any errors encountered copying files in the tree will be logged but
/// will not cause a short-circuit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_directory(
    progress_bar: Option<&ProgressBar>,
    root: &Path,
    target: &Path,
    options: &Options,
    symlinked_files: &mut HashSet<FileInformation>,
    copied_destinations: &HashSet<PathBuf>,
    copied_files: &mut HashMap<FileInformation, PathBuf>,
    source_in_command_line: bool,
) -> CopyResult<()> {
    // if no-dereference is enabled and this is a symlink, copy it as a file
    if !options.dereference(source_in_command_line) && root.is_symlink() {
        return copy_file(
            progress_bar,
            root,
            target,
            options,
            symlinked_files,
            copied_destinations,
            copied_files,
            source_in_command_line,
        );
    }

    if !options.recursive {
        return Err(translate!("cp-error-omitting-directory", "dir" => root.quote()).into());
    }

    // check if root is a prefix of target
    if path_has_prefix(target, root)? {
        return Err(translate!("cp-error-cannot-copy-directory-into-itself", "source" => root.quote(), "dest" => target.join(root.file_name().unwrap()).quote())
        .into());
    }

    // Check if we need safe traversal for long paths
    #[cfg(unix)]
    return safe_copy_directory(
        progress_bar,
        root,
        target,
        options,
        symlinked_files,
        copied_destinations,
        copied_files,
        source_in_command_line,
    );

    #[cfg(not(unix))]
    return walkdir_copy_directory(
        progress_bar,
        root,
        target,
        options,
        symlinked_files,
        copied_destinations,
        copied_files,
        source_in_command_line,
    );
}

/// Decide whether the second path is a prefix of the first.
///
/// This function canonicalizes the paths via
/// [`uucore::fs::canonicalize`] before comparing.
///
/// # Errors
///
/// If there is an error determining the canonical, absolute form of
/// either path.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(path_has_prefix(Path::new("/usr/bin"), Path::new("/usr")))
/// assert!(!path_has_prefix(Path::new("/usr"), Path::new("/usr/bin")))
/// assert!(!path_has_prefix(Path::new("/usr/bin"), Path::new("/var/log")))
/// ```
pub fn path_has_prefix(p1: &Path, p2: &Path) -> io::Result<bool> {
    let pathbuf1 = canonicalize(p1, MissingHandling::Normal, ResolveMode::Logical)?;
    let pathbuf2 = canonicalize(p2, MissingHandling::Normal, ResolveMode::Logical)?;

    Ok(pathbuf1.starts_with(pathbuf2))
}

/// Builds a directory at the specified path with the given options.
///
/// # Notes
/// - If `copy_attributes_from` is `Some`, the new directory's attributes will be
///   copied from the provided file. Otherwise, the new directory will have the default
///   attributes for the current user.
/// - This method excludes certain permissions if ownership or special mode bits could
///   potentially change. (See `test_dir_perm_race_with_preserve_mode_and_ownership`)
/// - The `recursive` flag determines whether parent directories should be created
///   if they do not already exist.
// we need to allow unused_variable since `options` might be unused in non unix systems
#[allow(unused_variables)]
fn build_dir(
    path: &PathBuf,
    recursive: bool,
    options: &Options,
    copy_attributes_from: Option<&Path>,
) -> CopyResult<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);

    // To prevent unauthorized access before the folder is ready,
    // exclude certain permissions if ownership or special mode bits
    // could potentially change.
    #[cfg(unix)]
    {
        use crate::Preserve;
        use std::os::unix::fs::PermissionsExt;

        // we need to allow trivial casts here because some systems like linux have u32 constants in
        // in libc while others don't.
        #[allow(clippy::unnecessary_cast)]
        let mut excluded_perms = if matches!(options.attributes.ownership, Preserve::Yes { .. }) {
            libc::S_IRWXG | libc::S_IRWXO // exclude rwx for group and other
        } else if matches!(options.attributes.mode, Preserve::Yes { .. }) {
            libc::S_IWGRP | libc::S_IWOTH //exclude w for group and other
        } else {
            0
        } as u32;

        let (umask, target_mode) = if let Some(path) = copy_attributes_from {
            if matches!(options.attributes.mode, Preserve::Yes { .. }) {
                // For directories, get the source mode to preserve permissions
                let source_mode = if options.dereference && fs::symlink_metadata(path)?.is_dir() {
                    fs::metadata(path)?.permissions().mode()
                } else {
                    fs::symlink_metadata(path)?.permissions().mode()
                };
                // Use the source mode directly, but still apply excluded permissions
                (0, source_mode & 0o777)
            } else {
                // Don't preserve mode
                let umask = uucore::mode::get_umask();
                (umask, (0o777 & !umask) | excluded_perms)
            }
        } else {
            (uucore::mode::get_umask(), 0o777)
        };

        excluded_perms |= umask;
        let mode = !excluded_perms & 0o777;
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, mode);
    }

    builder.create(path)?;

    Ok(())
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
fn safe_copy_directory(
    progress_bar: Option<&ProgressBar>,
    root: &Path,
    target: &Path,
    options: &Options,
    symlinked_files: &mut HashSet<FileInformation>,
    copied_destinations: &HashSet<PathBuf>,
    copied_files: &mut HashMap<FileInformation, PathBuf>,
    _source_in_command_line: bool,
) -> CopyResult<()> {
    // Handle --parents mode
    let tmp = if options.parents {
        if let Some(parent) = root.parent() {
            let new_target = target.join(parent);
            build_dir(&new_target, true, options, None)?;
            if options.verbose {
                for (x, y) in aligned_ancestors(root, &target.join(root)) {
                    println!("{} -> {}", x.display(), y.display());
                }
            }
            new_target
        } else {
            target.to_path_buf()
        }
    } else {
        target.to_path_buf()
    };
    let target = tmp.as_path();

    // Collect context information
    let context = match Context::new(root, target) {
        Ok(c) => c,
        Err(e) => {
            return Err(translate!("cp-error-failed-get-current-dir", "error" => e).into());
        }
    };

    // Create root directory first
    let entry = Entry::new(&context, root, options.no_target_dir)?;
    copy_direntry(
        progress_bar,
        entry.clone(),
        options,
        symlinked_files,
        false, // preserve_hard_links not used for root
        copied_destinations,
        copied_files,
    )?;

    // Keep track of directories needing permission fixes
    let mut dirs_needing_permissions: Vec<(PathBuf, PathBuf)> = Vec::new();
    dirs_needing_permissions.push((entry.source_absolute, entry.local_to_target));

    // Start safe traversal
    match DirFd::open(root) {
        Ok(dir_fd) => {
            safe_copy_dir_recursive(
                progress_bar,
                &dir_fd,
                root,
                &context,
                options,
                symlinked_files,
                copied_destinations,
                copied_files,
                &mut dirs_needing_permissions,
            )?;
        }
        Err(e) => {
            return Err(CpError::IoErrContext(
                e,
                format!("failed to open directory '{}'", root.display()),
            ));
        }
    }

    // Fix permissions for all directories we created
    for (source_abs, local_to_target) in dirs_needing_permissions.into_iter().rev() {
        copy_attributes(&source_abs, &local_to_target, &options.attributes)?;
    }

    // Also fix permissions for parent directories,
    // if we were asked to create them.
    if options.parents {
        let dest = target.join(root.file_name().unwrap());
        for (x, y) in aligned_ancestors(root, dest.as_path()) {
            if let Ok(src) = canonicalize(x, MissingHandling::Normal, ResolveMode::Physical) {
                copy_attributes(&src, y, &options.attributes)?;
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
fn safe_copy_dir_recursive(
    progress_bar: Option<&ProgressBar>,
    dir_fd: &DirFd,
    current_dir_path: &Path,
    context: &Context,
    options: &Options,
    symlinked_files: &mut HashSet<FileInformation>,
    copied_destinations: &HashSet<PathBuf>,
    copied_files: &mut HashMap<FileInformation, PathBuf>,
    dirs_needing_permissions: &mut Vec<(PathBuf, PathBuf)>,
) -> CopyResult<()> {
    // Read directory entries using safe traversal
    let entries = match dir_fd.read_dir() {
        Ok(entries) => entries,
        Err(e) => {
            return Err(CpError::IoErrContext(
                e,
                translate!("cp-error-failed-to-read-directory", "path" => current_dir_path.display()),
            ));
        }
    };

    for entry_name in entries {
        // Construct the real filesystem path for this entry
        let entry_path = current_dir_path.join(&entry_name);
        let entry_display_path = entry_path.display().to_string();

        // Get file stats to determine type
        let follow_symlinks = options.dereference;
        let entry_stat = match dir_fd.stat_at(&entry_name, follow_symlinks) {
            Ok(stat) => stat,
            Err(e) => {
                // Skip entries we can't stat, similar to WalkDir behavior
                show!(uio_error!(
                    e,
                    "{}",
                    translate!("cp-error-cannot-stat", "source" => entry_display_path),
                ));
                continue;
            }
        };

        // Create Entry for this item using the real filesystem path
        let entry = Entry::new(context, &entry_path, options.no_target_dir)?;

        // Copy this entry
        copy_direntry(
            progress_bar,
            entry.clone(),
            options,
            symlinked_files,
            false, // preserve_hard_links not used in recursive calls
            copied_destinations,
            copied_files,
        )?;

        // If it's a directory, add to permissions list and recurse
        #[allow(clippy::unnecessary_cast)]
        let is_dir = (entry_stat.st_mode as u32 & libc::S_IFMT as u32) == libc::S_IFDIR as u32;
        if is_dir {
            dirs_needing_permissions.push((entry.source_absolute, entry.local_to_target));

            // Recurse into subdirectory
            match dir_fd.open_subdir(&entry_name) {
                Ok(subdir_fd) => {
                    safe_copy_dir_recursive(
                        progress_bar,
                        &subdir_fd,
                        &entry_path,
                        context,
                        options,
                        symlinked_files,
                        copied_destinations,
                        copied_files,
                        dirs_needing_permissions,
                    )?;
                }
                Err(e) => {
                    show!(uio_error!(
                        e,
                        "{}",
                        translate!("cp-error-cannot-read-directory", "dir" => entry_display_path),
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn walkdir_copy_directory(
    progress_bar: Option<&ProgressBar>,
    root: &Path,
    target: &Path,
    options: &Options,
    symlinked_files: &mut HashSet<FileInformation>,
    copied_destinations: &HashSet<PathBuf>,
    copied_files: &mut HashMap<FileInformation, PathBuf>,
    _source_in_command_line: bool,
) -> CopyResult<()> {
    let context = Context::new(root, target)?;

    // Handle --parents mode
    let tmp = if options.parents {
        if let Some(parent) = root.parent() {
            let new_target = target.join(parent);
            build_dir(&new_target, true, options, None)?;
            if options.verbose {
                for (x, y) in aligned_ancestors(root, &target.join(root)) {
                    println!("{} -> {}", x.display(), y.display());
                }
            }
            new_target
        } else {
            target.to_path_buf()
        }
    } else {
        target.to_path_buf()
    };

    let target = &tmp;
    let preserve_hard_links = options.preserve_hard_links();

    let walker = WalkDir::new(root).follow_links(options.dereference);
    let mut errors = Vec::new();

    for entry in walker {
        match entry {
            Ok(entry) => {
                let entry_copy = Entry::from_walkdir(&context, &entry, options.no_target_dir)?;

                if let Err(err) = copy_direntry(
                    progress_bar,
                    entry_copy,
                    options,
                    symlinked_files,
                    preserve_hard_links,
                    copied_destinations,
                    copied_files,
                ) {
                    // Store the error but continue processing other entries
                    errors.push(err);
                }
            }
            Err(err) => {
                show_error!(
                    "{}",
                    translate!(
                        "cp-error-cannot-access",
                        "path" => err.path().unwrap_or(root).display().to_string(),
                        "error" => err.to_string()
                    )
                );
            }
        }
    }

    // Also fix permissions for parent directories,
    // if we were asked to create them.
    if options.parents {
        let dest = target.join(root.file_name().unwrap());
        for (x, y) in aligned_ancestors(root, dest.as_path()) {
            if let Ok(src) = canonicalize(x, MissingHandling::Normal, ResolveMode::Physical) {
                copy_attributes(&src, y, &options.attributes)?;
            }
        }
    }

    // Return the first error if any occurred
    if let Some(err) = errors.into_iter().next() {
        Err(err)
    } else {
        Ok(())
    }
}
