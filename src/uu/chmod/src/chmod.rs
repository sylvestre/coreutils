// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) Chmoder cmode fmode fperm fref ugoa RFILE RFILE's

use clap::{Arg, ArgAction, Command};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use thiserror::Error;
use uucore::LocalizedCommand;
use uucore::display::Quotable;
use uucore::error::{ExitCode, UError, UResult, USimpleError, UUsageError, set_exit_code};
use uucore::fs::display_permissions_unix;
use uucore::libc::mode_t;
use uucore::mode;
use uucore::perms::{TraverseSymlinks, configure_symlink_and_recursion};
#[cfg(unix)]
use uucore::safe_traversal::DirFd;
use uucore::{format_usage, show, show_error};

use uucore::translate;

#[derive(Debug, Error)]
enum ChmodError {
    #[error("{}", translate!("chmod-error-cannot-stat", "file" => _0.quote()))]
    CannotStat(String),
    #[error("{}", translate!("chmod-error-dangling-symlink", "file" => _0.quote()))]
    DanglingSymlink(String),
    #[error("{}", translate!("chmod-error-no-such-file", "file" => _0.quote()))]
    NoSuchFile(String),
    #[error("{}", translate!("chmod-error-preserve-root", "file" => _0.quote()))]
    PreserveRoot(String),
    #[error("{}", translate!("chmod-error-permission-denied", "file" => _0.quote()))]
    PermissionDenied(String),
    #[error("{}", translate!("chmod-error-new-permissions", "file" => _0.clone(), "actual" => _1.clone(), "expected" => _2.clone()))]
    NewPermissions(String, String, String),
}

impl UError for ChmodError {}

mod options {
    pub const HELP: &str = "help";
    pub const CHANGES: &str = "changes";
    pub const QUIET: &str = "quiet"; // visible_alias("silent")
    pub const VERBOSE: &str = "verbose";
    pub const NO_PRESERVE_ROOT: &str = "no-preserve-root";
    pub const PRESERVE_ROOT: &str = "preserve-root";
    pub const REFERENCE: &str = "RFILE";
    pub const RECURSIVE: &str = "recursive";
    pub const MODE: &str = "MODE";
    pub const FILE: &str = "FILE";
}

/// Extract negative modes (starting with '-') from the rest of the arguments.
///
/// This is mainly required for GNU compatibility, where "non-positional negative" modes are used
/// as the actual positional MODE. Some examples of these cases are:
/// * "chmod -w -r file", which is the same as "chmod -w,-r file"
/// * "chmod -w file -r", which is the same as "chmod -w,-r file"
///
/// These can currently not be handled by clap.
/// Therefore it might be possible that a pseudo MODE is inserted to pass clap parsing.
/// The pseudo MODE is later replaced by the extracted (and joined) negative modes.
fn extract_negative_modes(mut args: impl uucore::Args) -> (Option<String>, Vec<OsString>) {
    // we look up the args until "--" is found
    // "-mode" will be extracted into parsed_cmode_vec
    let (parsed_cmode_vec, pre_double_hyphen_args): (Vec<OsString>, Vec<OsString>) =
        args.by_ref().take_while(|a| a != "--").partition(|arg| {
            let arg = if let Some(arg) = arg.to_str() {
                arg.to_string()
            } else {
                return false;
            };
            arg.len() >= 2
                && arg.starts_with('-')
                && matches!(
                    arg.chars().nth(1).unwrap(),
                    'r' | 'w' | 'x' | 'X' | 's' | 't' | 'u' | 'g' | 'o' | '0'..='7'
                )
        });

    let mut clean_args = Vec::new();
    if !parsed_cmode_vec.is_empty() {
        // we need a pseudo cmode for clap, which won't be used later.
        // this is required because clap needs the default "chmod MODE FILE" scheme.
        clean_args.push("w".into());
    }
    clean_args.extend(pre_double_hyphen_args);

    if let Some(arg) = args.next() {
        // as there is still something left in the iterator, we previously consumed the "--"
        // -> add it to the args again
        clean_args.push("--".into());
        clean_args.push(arg);
    }
    clean_args.extend(args);

    let parsed_cmode = Some(
        parsed_cmode_vec
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect::<Vec<&str>>()
            .join(","),
    )
    .filter(|s| !s.is_empty());
    (parsed_cmode, clean_args)
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let (parsed_cmode, args) = extract_negative_modes(args.skip(1)); // skip binary name
    let matches = uu_app()
        .after_help(translate!("chmod-after-help"))
        .get_matches_from_localized(args);

    let changes = matches.get_flag(options::CHANGES);
    let quiet = matches.get_flag(options::QUIET);
    let verbose = matches.get_flag(options::VERBOSE);
    let preserve_root = matches.get_flag(options::PRESERVE_ROOT);
    let fmode = match matches.get_one::<OsString>(options::REFERENCE) {
        Some(fref) => match fs::metadata(fref) {
            Ok(meta) => Some(meta.mode() & 0o7777),
            Err(_) => {
                return Err(ChmodError::CannotStat(fref.to_string_lossy().to_string()).into());
            }
        },
        None => None,
    };

    let modes = matches.get_one::<String>(options::MODE);
    let cmode = if let Some(parsed_cmode) = parsed_cmode {
        parsed_cmode
    } else {
        modes.unwrap().to_string() // modes is required
    };
    let mut files: Vec<OsString> = matches
        .get_many::<OsString>(options::FILE)
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let cmode = if fmode.is_some() {
        // "--reference" and MODE are mutually exclusive
        // if "--reference" was used MODE needs to be interpreted as another FILE
        // it wasn't possible to implement this behavior directly with clap
        files.push(OsString::from(cmode));
        None
    } else {
        Some(cmode)
    };

    if files.is_empty() {
        return Err(UUsageError::new(
            1,
            translate!("chmod-error-missing-operand"),
        ));
    }

    let (recursive, dereference, traverse_symlinks) =
        configure_symlink_and_recursion(&matches, TraverseSymlinks::First)?;

    let chmoder = Chmoder {
        changes,
        quiet,
        verbose,
        preserve_root,
        recursive,
        fmode,
        cmode,
        traverse_symlinks,
        dereference,
    };

    chmoder.chmod(&files)
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(uucore::crate_version!())
        .help_template(uucore::localized_help_template(uucore::util_name()))
        .about(translate!("chmod-about"))
        .override_usage(format_usage(&translate!("chmod-usage")))
        .args_override_self(true)
        .infer_long_args(true)
        .no_binary_name(true)
        .disable_help_flag(true)
        .arg(
            Arg::new(options::HELP)
                .long(options::HELP)
                .help(translate!("chmod-help-print-help"))
                .action(ArgAction::Help),
        )
        .arg(
            Arg::new(options::CHANGES)
                .long(options::CHANGES)
                .short('c')
                .help(translate!("chmod-help-changes"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::QUIET)
                .long(options::QUIET)
                .visible_alias("silent")
                .short('f')
                .help(translate!("chmod-help-quiet"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::VERBOSE)
                .long(options::VERBOSE)
                .short('v')
                .help(translate!("chmod-help-verbose"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NO_PRESERVE_ROOT)
                .long(options::NO_PRESERVE_ROOT)
                .help(translate!("chmod-help-no-preserve-root"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::PRESERVE_ROOT)
                .long(options::PRESERVE_ROOT)
                .help(translate!("chmod-help-preserve-root"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::RECURSIVE)
                .long(options::RECURSIVE)
                .short('R')
                .help(translate!("chmod-help-recursive"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::REFERENCE)
                .long("reference")
                .value_hint(clap::ValueHint::FilePath)
                .value_parser(clap::value_parser!(OsString))
                .help(translate!("chmod-help-reference")),
        )
        .arg(
            Arg::new(options::MODE).required_unless_present(options::REFERENCE),
            // It would be nice if clap could parse with delimiter, e.g. "g-x,u+x",
            // however .multiple_occurrences(true) cannot be used here because FILE already needs that.
            // Only one positional argument with .multiple_occurrences(true) set is allowed per command
        )
        .arg(
            Arg::new(options::FILE)
                .required_unless_present(options::MODE)
                .action(ArgAction::Append)
                .value_hint(clap::ValueHint::AnyPath)
                .value_parser(clap::value_parser!(OsString)),
        )
        // Add common arguments with chgrp, chown & chmod
        .args(uucore::perms::common_args())
}

struct Chmoder {
    changes: bool,
    quiet: bool,
    verbose: bool,
    preserve_root: bool,
    recursive: bool,
    fmode: Option<u32>,
    cmode: Option<String>,
    traverse_symlinks: TraverseSymlinks,
    dereference: bool,
}

impl Chmoder {
    fn chmod(&self, files: &[OsString]) -> UResult<()> {
        let mut r = Ok(());
        let mut general_permission_denied = false;

        for filename in files {
            let file = Path::new(filename);
            if !file.exists() {
                if file.is_symlink() {
                    if !self.dereference && !self.recursive {
                        // The file is a symlink and we should not follow it
                        // Don't try to change the mode of the symlink itself
                        continue;
                    }
                    if self.recursive && self.traverse_symlinks == TraverseSymlinks::None {
                        continue;
                    }

                    if !self.quiet {
                        show!(ChmodError::DanglingSymlink(
                            filename.to_string_lossy().to_string()
                        ));
                        set_exit_code(1);
                    }

                    if self.verbose {
                        println!(
                            "{}",
                            translate!("chmod-verbose-failed-dangling", "file" => filename.to_string_lossy().quote())
                        );
                    }
                } else if !self.quiet {
                    show!(ChmodError::NoSuchFile(
                        filename.to_string_lossy().to_string()
                    ));
                }
                // GNU exits with exit code 1 even if -q or --quiet are passed
                // So we set the exit code, because it hasn't been set yet if `self.quiet` is true.
                set_exit_code(1);
                continue;
            } else if !self.dereference && file.is_symlink() {
                // The file is a symlink and we should not follow it
                // chmod 755 --no-dereference a/link
                // should not change the permissions in this case
                continue;
            }
            if self.recursive && self.preserve_root && file == Path::new("/") {
                return Err(ChmodError::PreserveRoot("/".to_string()).into());
            }
            if self.recursive {
                match self.chmod_recursive_internal(file, &mut general_permission_denied) {
                    Ok(()) => {}
                    Err(e) => {
                        if r.is_ok() {
                            r = Err(e);
                        }
                    }
                }
            } else {
                r = self.chmod_file(file).and(r);
            }
        }

        // After processing all files, emit general permission denied if needed
        if general_permission_denied && r.is_ok() {
            if !self.quiet {
                show_error!("Permission denied");
            }
            return Err(ExitCode::new(1));
        }

        r
    }

    fn chmod_recursive_internal(
        &self,
        file_path: &Path,
        general_permission_denied: &mut bool,
    ) -> UResult<()> {
        #[cfg(unix)]
        return self.safe_chmod_recursive_internal(file_path, general_permission_denied);

        #[cfg(not(unix))]
        {
            // On non-Unix systems, just process the file itself (no recursion for now)
            self.chmod_file(file_path)
        }
    }

    fn should_follow_symlink(&self, is_command_line_arg: bool) -> bool {
        match self.traverse_symlinks {
            TraverseSymlinks::All => true,
            TraverseSymlinks::First => is_command_line_arg,
            TraverseSymlinks::None => false,
        }
    }

    fn chmod_file(&self, file: &Path) -> UResult<()> {
        self.chmod_file_internal(file, self.dereference)
    }

    fn chmod_file_internal(&self, file: &Path, dereference: bool) -> UResult<()> {
        use uucore::{mode::get_umask, perms::get_metadata};

        let metadata = get_metadata(file, dereference);

        let fperm = match metadata {
            Ok(meta) => meta.mode() & 0o7777,
            Err(err) => {
                // Handle dangling symlinks or other errors
                return if file.is_symlink() && !dereference {
                    if self.verbose {
                        println!(
                            "neither symbolic link {} nor referent has been changed",
                            file.quote()
                        );
                    }
                    Ok(()) // Skip dangling symlinks
                } else if err.kind() == std::io::ErrorKind::PermissionDenied {
                    // These two filenames would normally be conditionally
                    // quoted, but GNU's tests expect them to always be quoted
                    Err(ChmodError::PermissionDenied(file.to_string_lossy().to_string()).into())
                } else {
                    Err(ChmodError::CannotStat(file.to_string_lossy().to_string()).into())
                };
            }
        };

        // Determine the new permissions to apply
        match self.fmode {
            Some(mode) => self.change_file(fperm, mode, file)?,
            None => {
                let cmode_unwrapped = self.cmode.clone().unwrap();
                let mut new_mode = fperm;
                let mut naively_expected_new_mode = new_mode;
                for mode in cmode_unwrapped.split(',') {
                    let result = if mode.chars().any(|c| c.is_ascii_digit()) {
                        mode::parse_numeric(new_mode, mode, file.is_dir()).map(|v| (v, v))
                    } else {
                        mode::parse_symbolic(new_mode, mode, get_umask(), file.is_dir()).map(|m| {
                            // calculate the new mode as if umask was 0
                            let naive_mode = mode::parse_symbolic(
                                naively_expected_new_mode,
                                mode,
                                0,
                                file.is_dir(),
                            )
                            .unwrap(); // we know that mode must be valid, so this cannot fail
                            (m, naive_mode)
                        })
                    };

                    match result {
                        Ok((mode, naive_mode)) => {
                            new_mode = mode;
                            naively_expected_new_mode = naive_mode;
                        }
                        Err(f) => {
                            return if self.quiet {
                                Err(ExitCode::new(1))
                            } else {
                                Err(USimpleError::new(1, f))
                            };
                        }
                    }
                }

                // Special handling for symlinks when not dereferencing
                if file.is_symlink() && !dereference {
                    // TODO: On most Unix systems, symlink permissions are ignored by the kernel,
                    // so changing them has no effect. We skip this operation for compatibility.
                    // Note that "chmod without dereferencing" effectively does nothing on symlinks.
                    if self.verbose {
                        println!(
                            "neither symbolic link {} nor referent has been changed",
                            file.quote()
                        );
                    }
                } else {
                    self.change_file(fperm, new_mode, file)?;
                }
                // if a permission would have been removed if umask was 0, but it wasn't because umask was not 0, print an error and fail
                if (new_mode & !naively_expected_new_mode) != 0 {
                    return Err(ChmodError::NewPermissions(
                        file.to_string_lossy().to_string(),
                        display_permissions_unix(new_mode as mode_t, false),
                        display_permissions_unix(naively_expected_new_mode as mode_t, false),
                    )
                    .into());
                }
            }
        }

        Ok(())
    }

    #[cfg(unix)]
    fn change_file(&self, fperm: u32, mode: u32, file: &Path) -> Result<(), i32> {
        if fperm == mode {
            if self.verbose && !self.changes {
                println!(
                    "mode of {} retained as {fperm:04o} ({})",
                    file.quote(),
                    display_permissions_unix(fperm as mode_t, false),
                );
            }
            Ok(())
        } else if let Err(err) = fs::set_permissions(file, fs::Permissions::from_mode(mode)) {
            if !self.quiet {
                show_error!("{err}");
            }
            if self.verbose {
                println!(
                    "failed to change mode of file {} from {fperm:04o} ({}) to {mode:04o} ({})",
                    file.quote(),
                    display_permissions_unix(fperm as mode_t, false),
                    display_permissions_unix(mode as mode_t, false)
                );
            }
            Err(1)
        } else {
            if self.verbose || self.changes {
                println!(
                    "mode of {} changed from {fperm:04o} ({}) to {mode:04o} ({})",
                    file.quote(),
                    display_permissions_unix(fperm as mode_t, false),
                    display_permissions_unix(mode as mode_t, false)
                );
            }
            Ok(())
        }
    }

    #[cfg(unix)]
    fn safe_chmod_recursive_internal(
        &self,
        file_path: &Path,
        general_permission_denied: &mut bool,
    ) -> UResult<()> {
        // First, apply chmod to the current file/directory
        let mut result = self.chmod_file(file_path);

        // If it's a directory, try to recurse into it
        if file_path.is_dir() {
            // After potentially changing permissions, try to recurse
            match DirFd::open(file_path) {
                Ok(dir_fd) => {
                    // Successfully opened directory, walk its contents
                    result = self.safe_walk_dir(&dir_fd, file_path, true).and(result);
                }
                Err(_) => {
                    // Failed to open directory, check if it's a permission error
                    if let Err(e) = fs::read_dir(file_path) {
                        if e.kind() == std::io::ErrorKind::PermissionDenied {
                            // Mark that we had a general permission denied error
                            *general_permission_denied = true;
                            return Ok(());
                        }
                    }
                }
            }
        }

        result
    }

    #[cfg(unix)]
    fn safe_walk_dir(
        &self,
        dir_fd: &DirFd,
        base_path: &Path,
        _is_command_line_arg: bool,
    ) -> UResult<()> {
        let mut r = Ok(());

        let Ok(entries) = dir_fd.read_dir() else {
            return Ok(());
        };

        for entry_name in entries {
            let entry_path = base_path.join(entry_name.to_string_lossy().as_ref());
            let entry_display = Self::format_path_display(&entry_path);

            let should_follow_symlink = self.should_follow_symlink(false);

            let entry_stat = match dir_fd.stat_at(&entry_name, should_follow_symlink) {
                Ok(stat) => stat,
                Err(err) => {
                    // Check if it's a permission denied error
                    if err.kind() == std::io::ErrorKind::PermissionDenied {
                        if !self.quiet {
                            show_error!("{}: Permission denied", entry_display.quote());
                        }
                        return Err(ExitCode::new(1));
                    }
                    continue;
                }
            };

            #[allow(clippy::unnecessary_cast)]
            let is_dir = (entry_stat.st_mode as u32 & uucore::libc::S_IFMT as u32)
                == uucore::libc::S_IFDIR as u32;

            r = self
                .safe_chmod_file_at(dir_fd, &entry_name, &entry_display, should_follow_symlink)
                .and(r);

            if is_dir {
                if let Ok(subdir_fd) = dir_fd.open_subdir(&entry_name) {
                    r = self.safe_walk_dir(&subdir_fd, &entry_path, false).and(r);
                } else {
                    // Check if it's a permission error and propagate it
                    if let Err(e) = fs::read_dir(&entry_path) {
                        if e.kind() == std::io::ErrorKind::PermissionDenied {
                            // Check if we need to go deeper - if there are nested paths we can't access
                            // For now, try to continue and let deeper errors surface
                            if let Err(e) = fs::metadata(&entry_path) {
                                if e.kind() == std::io::ErrorKind::PermissionDenied {
                                    r = Err(ChmodError::PermissionDenied(
                                        entry_path.display().to_string(),
                                    )
                                    .into());
                                }
                            }
                        }
                    }
                }
            }
        }

        r
    }

    fn format_path_display(path: &Path) -> String {
        let path_str = path.to_string_lossy();
        if path_str.len() > 1000 {
            format!(
                ".../{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            path_str.to_string()
        }
    }

    #[cfg(unix)]
    fn safe_chmod_file_at(
        &self,
        dir_fd: &DirFd,
        entry_name: &OsStr,
        display_path: &str,
        dereference: bool,
    ) -> UResult<()> {
        use uucore::mode::get_umask;

        // Get metadata for the file
        let metadata = match dir_fd.stat_at(entry_name, dereference) {
            Ok(stat) => stat,
            Err(_err) => {
                // Handle errors similar to chmod_file_internal
                return if self.quiet {
                    Ok(())
                } else {
                    Err(ChmodError::CannotStat(display_path.to_string()).into())
                };
            }
        };

        // Allow unnecessary cast because st_mode has different types on different platforms
        // (u16 on macOS, u32 on Linux)
        #[allow(clippy::unnecessary_cast)]
        let fperm = (metadata.st_mode & 0o7777) as u32;
        #[allow(clippy::unnecessary_cast)]
        let is_dir =
            (metadata.st_mode as u32 & uucore::libc::S_IFMT as u32) == uucore::libc::S_IFDIR as u32;
        #[allow(clippy::unnecessary_cast)]
        let is_symlink =
            (metadata.st_mode as u32 & uucore::libc::S_IFMT as u32) == uucore::libc::S_IFLNK as u32;

        // If it's a symlink and we're not following symlinks, skip it
        // (symlink permissions can't be changed on most Unix systems)
        if is_symlink && !dereference {
            return Ok(());
        }

        // Determine the new permissions to apply
        match self.fmode {
            Some(mode) => self.safe_change_file_at(
                dir_fd,
                entry_name,
                display_path,
                fperm,
                mode,
                dereference,
            )?,
            None => {
                let cmode_unwrapped = self.cmode.clone().unwrap();
                let mut new_mode = fperm;
                let mut naively_expected_new_mode = new_mode;

                for mode in cmode_unwrapped.split(',') {
                    let result = if mode.chars().any(|c| c.is_ascii_digit()) {
                        mode::parse_numeric(new_mode, mode, is_dir).map(|v| (v, v))
                    } else {
                        mode::parse_symbolic(new_mode, mode, get_umask(), is_dir).map(|m| {
                            let naive_mode =
                                mode::parse_symbolic(naively_expected_new_mode, mode, 0, is_dir)
                                    .unwrap();
                            (m, naive_mode)
                        })
                    };

                    match result {
                        Ok((mode, naive_mode)) => {
                            new_mode = mode;
                            naively_expected_new_mode = naive_mode;
                        }
                        Err(f) => {
                            return if self.quiet {
                                Err(ExitCode::new(1))
                            } else {
                                Err(USimpleError::new(1, f))
                            };
                        }
                    }
                }

                self.safe_change_file_at(
                    dir_fd,
                    entry_name,
                    display_path,
                    fperm,
                    new_mode,
                    dereference,
                )?;

                // Check for permission issues with umask
                if (new_mode & !naively_expected_new_mode) != 0 {
                    return Err(ChmodError::NewPermissions(
                        display_path.to_string(),
                        display_permissions_unix(new_mode as mode_t, false),
                        display_permissions_unix(naively_expected_new_mode as mode_t, false),
                    )
                    .into());
                }
            }
        }

        Ok(())
    }

    #[cfg(unix)]
    fn safe_change_file_at(
        &self,
        dir_fd: &DirFd,
        entry_name: &OsStr,
        display_path: &str,
        fperm: u32,
        mode: u32,
        follow_symlinks: bool,
    ) -> Result<(), i32> {
        if fperm == mode {
            if self.verbose && !self.changes {
                println!(
                    "mode of {} retained as {fperm:04o} ({})",
                    display_path,
                    display_permissions_unix(fperm as mode_t, false),
                );
            }
            Ok(())
        } else if let Err(_err) = dir_fd.chmod_at(entry_name, mode, follow_symlinks) {
            if !self.quiet {
                show_error!("failed to change mode of {}", display_path);
            }
            if self.verbose {
                println!(
                    "failed to change mode of file {} from {fperm:04o} ({}) to {mode:04o} ({})",
                    display_path,
                    display_permissions_unix(fperm as mode_t, false),
                    display_permissions_unix(mode as mode_t, false)
                );
            }
            Err(1)
        } else {
            if self.verbose || self.changes {
                println!(
                    "mode of {} changed from {fperm:04o} ({}) to {mode:04o} ({})",
                    display_path,
                    display_permissions_unix(fperm as mode_t, false),
                    display_permissions_unix(mode as mode_t, false)
                );
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_negative_modes() {
        // "chmod -w -r file" becomes "chmod -w,-r file". clap does not accept "-w,-r" as MODE.
        // Therefore, "w" is added as pseudo mode to pass clap.
        let (c, a) = extract_negative_modes(["-w", "-r", "file"].iter().map(OsString::from));
        assert_eq!(c, Some("-w,-r".to_string()));
        assert_eq!(a, ["w", "file"]);

        // "chmod -w file -r" becomes "chmod -w,-r file". clap does not accept "-w,-r" as MODE.
        // Therefore, "w" is added as pseudo mode to pass clap.
        let (c, a) = extract_negative_modes(["-w", "file", "-r"].iter().map(OsString::from));
        assert_eq!(c, Some("-w,-r".to_string()));
        assert_eq!(a, ["w", "file"]);

        // "chmod -w -- -r file" becomes "chmod -w -r file", where "-r" is interpreted as file.
        // Again, "w" is needed as pseudo mode.
        let (c, a) = extract_negative_modes(["-w", "--", "-r", "f"].iter().map(OsString::from));
        assert_eq!(c, Some("-w".to_string()));
        assert_eq!(a, ["w", "--", "-r", "f"]);

        // "chmod -- -r file" becomes "chmod -r file".
        let (c, a) = extract_negative_modes(["--", "-r", "file"].iter().map(OsString::from));
        assert_eq!(c, None);
        assert_eq!(a, ["--", "-r", "file"]);
    }
}
