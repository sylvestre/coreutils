// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) rwxr sourcepath targetpath Isnt uioerror

mod mode;

use clap::{Arg, ArgAction, ArgMatches, Command};
use file_diff::diff;
use filetime::{FileTime, set_file_times};
use std::fmt::Debug;
use std::fs::File;
use std::fs::{self, metadata};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::process;
use thiserror::Error;
use uucore::backup_control::{self, BackupMode};
use uucore::buf_copy::copy_stream;
use uucore::display::Quotable;
use uucore::entries::{grp2gid, usr2uid};
use uucore::error::{FromIo, UError, UResult, UUsageError};
use uucore::fs::dir_strip_dot_for_creation;
use uucore::mode::get_umask;
use uucore::perms::{Verbosity, VerbosityLevel, wrap_chown};
use uucore::process::{getegid, geteuid};
#[cfg(feature = "selinux")]
use uucore::selinux::{contexts_differ, set_selinux_security_context};
use uucore::translate;
use uucore::{format_usage, show, show_error, show_if_err};

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt};
#[cfg(unix)]
use std::os::unix::prelude::OsStrExt;

const DEFAULT_MODE: u32 = 0o755;
const DEFAULT_STRIP_PROGRAM: &str = "strip";

#[allow(dead_code)]
pub struct Behavior {
    main_function: MainFunction,
    specified_mode: Option<u32>,
    backup_mode: BackupMode,
    suffix: String,
    owner_id: Option<u32>,
    group_id: Option<u32>,
    verbose: bool,
    preserve_timestamps: bool,
    compare: bool,
    strip: bool,
    strip_program: String,
    create_leading: bool,
    target_dir: Option<String>,
    no_target_dir: bool,
    preserve_context: bool,
    context: Option<String>,
}

#[derive(Error, Debug)]
enum InstallError {
    #[error("{}", translate!("install-error-dir-needs-arg", "util_name" => uucore::util_name()))]
    DirNeedsArg,

    #[error("{}", translate!("install-error-create-dir-failed", "path" => .0.quote()))]
    CreateDirFailed(PathBuf, #[source] std::io::Error),

    #[error("{}", translate!("install-error-chmod-failed", "path" => .0.quote()))]
    ChmodFailed(PathBuf),

    #[error("{}", translate!("install-error-chown-failed", "path" => .0.quote(), "error" => .1.clone()))]
    ChownFailed(PathBuf, String),

    #[error("{}", translate!("install-error-invalid-target", "path" => .0.quote()))]
    InvalidTarget(PathBuf),

    #[error("{}", translate!("install-error-target-not-dir", "path" => .0.quote()))]
    TargetDirIsntDir(PathBuf),

    #[error("{}", translate!("install-error-backup-failed", "from" => .0.to_string_lossy(), "to" => .1.to_string_lossy()))]
    BackupFailed(PathBuf, PathBuf, #[source] std::io::Error),

    #[error("{}", translate!("install-error-install-failed", "from" => .0.to_string_lossy(), "to" => .1.to_string_lossy()))]
    InstallFailed(PathBuf, PathBuf, #[source] std::io::Error),

    #[error("{}", translate!("install-error-strip-failed", "error" => .0.clone()))]
    StripProgramFailed(String),

    #[error("{}", translate!("install-error-metadata-failed"))]
    MetadataFailed(#[source] std::io::Error),

    #[error("{}", translate!("install-error-invalid-user", "user" => .0.quote()))]
    InvalidUser(String),

    #[error("{}", translate!("install-error-invalid-group", "group" => .0.quote()))]
    InvalidGroup(String),

    #[error("{}", translate!("install-error-omitting-directory", "path" => .0.quote()))]
    OmittingDirectory(PathBuf),

    #[error("{}", translate!("install-error-not-a-directory", "path" => .0.quote()))]
    NotADirectory(PathBuf),

    #[error("{}", translate!("install-error-override-directory-failed", "dir" => .0.quote(), "file" => .1.quote()))]
    OverrideDirectoryFailed(PathBuf, PathBuf),

    #[error("{}", translate!("install-error-same-file", "file1" => .0.to_string_lossy(), "file2" => .1.to_string_lossy()))]
    SameFile(PathBuf, PathBuf),

    #[error("{}", translate!("install-error-extra-operand", "operand" => .0.quote(), "usage" => .1.clone()))]
    ExtraOperand(String, String),

    #[cfg(feature = "selinux")]
    #[error("{}", .0)]
    SelinuxContextFailed(String),
}

impl UError for InstallError {
    fn code(&self) -> i32 {
        1
    }

    fn usage(&self) -> bool {
        false
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum MainFunction {
    /// Create directories
    Directory,
    /// Install files to locations (primary functionality)
    Standard,
}

impl Behavior {
    /// Determine the mode for chmod after copy.
    pub fn mode(&self) -> u32 {
        self.specified_mode.unwrap_or(DEFAULT_MODE)
    }
}

static OPT_COMPARE: &str = "compare";
static OPT_DIRECTORY: &str = "directory";
static OPT_IGNORED: &str = "ignored";
static OPT_CREATE_LEADING: &str = "create-leading";
static OPT_GROUP: &str = "group";
static OPT_MODE: &str = "mode";
static OPT_OWNER: &str = "owner";
static OPT_PRESERVE_TIMESTAMPS: &str = "preserve-timestamps";
static OPT_STRIP: &str = "strip";
static OPT_STRIP_PROGRAM: &str = "strip-program";
static OPT_TARGET_DIRECTORY: &str = "target-directory";
static OPT_NO_TARGET_DIRECTORY: &str = "no-target-directory";
static OPT_VERBOSE: &str = "verbose";
static OPT_PRESERVE_CONTEXT: &str = "preserve-context";
static OPT_CONTEXT: &str = "context";

static ARG_FILES: &str = "files";

/// Main install utility function, called from main.rs.
///
/// Returns a program return code.
///
#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let paths: Vec<String> = matches
        .get_many::<String>(ARG_FILES)
        .map(|v| v.map(ToString::to_string).collect())
        .unwrap_or_default();

    let behavior = behavior(&matches)?;

    match behavior.main_function {
        MainFunction::Directory => directory(&paths, &behavior),
        MainFunction::Standard => standard(paths, &behavior),
    }
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(uucore::crate_version!())
        .about(translate!("install-about"))
        .override_usage(format_usage(&translate!("install-usage")))
        .infer_long_args(true)
        .args_override_self(true)
        .arg(backup_control::arguments::backup())
        .arg(backup_control::arguments::backup_no_args())
        .arg(
            Arg::new(OPT_IGNORED)
                .short('c')
                .help(translate!("install-help-ignored"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_COMPARE)
                .short('C')
                .long(OPT_COMPARE)
                .help(translate!("install-help-compare"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_DIRECTORY)
                .short('d')
                .long(OPT_DIRECTORY)
                .help(translate!("install-help-directory"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_CREATE_LEADING)
                .short('D')
                .help(translate!("install-help-create-leading"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_GROUP)
                .short('g')
                .long(OPT_GROUP)
                .help(translate!("install-help-group"))
                .value_name("GROUP"),
        )
        .arg(
            Arg::new(OPT_MODE)
                .short('m')
                .long(OPT_MODE)
                .help(translate!("install-help-mode"))
                .value_name("MODE"),
        )
        .arg(
            Arg::new(OPT_OWNER)
                .short('o')
                .long(OPT_OWNER)
                .help(translate!("install-help-owner"))
                .value_name("OWNER")
                .value_hint(clap::ValueHint::Username),
        )
        .arg(
            Arg::new(OPT_PRESERVE_TIMESTAMPS)
                .short('p')
                .long(OPT_PRESERVE_TIMESTAMPS)
                .help(translate!("install-help-preserve-timestamps"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_STRIP)
                .short('s')
                .long(OPT_STRIP)
                .help(translate!("install-help-strip"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_STRIP_PROGRAM)
                .long(OPT_STRIP_PROGRAM)
                .help(translate!("install-help-strip-program"))
                .value_name("PROGRAM")
                .value_hint(clap::ValueHint::CommandName),
        )
        .arg(backup_control::arguments::suffix())
        .arg(
            Arg::new(OPT_TARGET_DIRECTORY)
                .short('t')
                .long(OPT_TARGET_DIRECTORY)
                .help(translate!("install-help-target-directory"))
                .value_name("DIRECTORY")
                .value_hint(clap::ValueHint::DirPath),
        )
        .arg(
            Arg::new(OPT_NO_TARGET_DIRECTORY)
                .short('T')
                .long(OPT_NO_TARGET_DIRECTORY)
                .help(translate!("install-help-no-target-directory"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_VERBOSE)
                .short('v')
                .long(OPT_VERBOSE)
                .help(translate!("install-help-verbose"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_PRESERVE_CONTEXT)
                .short('P')
                .long(OPT_PRESERVE_CONTEXT)
                .help(translate!("install-help-preserve-context"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(OPT_CONTEXT)
                .short('Z')
                .long(OPT_CONTEXT)
                .help(translate!("install-help-context"))
                .value_name("CONTEXT")
                .value_parser(clap::value_parser!(String))
                .num_args(0..=1),
        )
        .arg(
            Arg::new(ARG_FILES)
                .action(ArgAction::Append)
                .num_args(1..)
                .value_hint(clap::ValueHint::AnyPath),
        )
}

/// Determine behavior, given command line arguments.
///
/// If successful, returns a filled-out Behavior struct.
///
/// # Errors
///
/// In event of failure, returns an integer intended as a program return code.
///
fn behavior(matches: &ArgMatches) -> UResult<Behavior> {
    let main_function = if matches.get_flag(OPT_DIRECTORY) {
        MainFunction::Directory
    } else {
        MainFunction::Standard
    };

    let considering_dir: bool = MainFunction::Directory == main_function;

    let specified_mode: Option<u32> = if matches.contains_id(OPT_MODE) {
        let x = matches.get_one::<String>(OPT_MODE).ok_or(1)?;
        Some(mode::parse(x, considering_dir, get_umask()).map_err(|err| {
            show_error!(
                "{}",
                translate!("install-error-invalid-mode", "error" => err)
            );
            1
        })?)
    } else {
        None
    };

    let backup_mode = backup_control::determine_backup_mode(matches)?;
    let target_dir = matches.get_one::<String>(OPT_TARGET_DIRECTORY).cloned();
    let no_target_dir = matches.get_flag(OPT_NO_TARGET_DIRECTORY);
    if target_dir.is_some() && no_target_dir {
        show_error!("{}", translate!("install-error-mutually-exclusive-target"));
        return Err(1.into());
    }

    let preserve_timestamps = matches.get_flag(OPT_PRESERVE_TIMESTAMPS);
    let compare = matches.get_flag(OPT_COMPARE);
    let strip = matches.get_flag(OPT_STRIP);
    if preserve_timestamps && compare {
        show_error!(
            "{}",
            translate!("install-error-mutually-exclusive-compare-preserve")
        );
        return Err(1.into());
    }
    if compare && strip {
        show_error!(
            "{}",
            translate!("install-error-mutually-exclusive-compare-strip")
        );
        return Err(1.into());
    }

    // Check if compare is used with non-permission mode bits
    if compare && specified_mode.is_some() {
        let mode = specified_mode.unwrap();
        let non_permission_bits = 0o7000; // setuid, setgid, sticky bits
        if mode & non_permission_bits != 0 {
            show_error!("{}", translate!("install-warning-compare-ignored"));
        }
    }

    let owner = matches
        .get_one::<String>(OPT_OWNER)
        .map_or("", |s| s.as_str())
        .to_string();

    let owner_id = if owner.is_empty() {
        None
    } else {
        match usr2uid(&owner) {
            Ok(u) => Some(u),
            Err(_) => return Err(InstallError::InvalidUser(owner.clone()).into()),
        }
    };

    let group = matches
        .get_one::<String>(OPT_GROUP)
        .map_or("", |s| s.as_str())
        .to_string();

    let group_id = if group.is_empty() {
        None
    } else {
        match grp2gid(&group) {
            Ok(g) => Some(g),
            Err(_) => return Err(InstallError::InvalidGroup(group.clone()).into()),
        }
    };

    let context = matches.get_one::<String>(OPT_CONTEXT).cloned();

    Ok(Behavior {
        main_function,
        specified_mode,
        backup_mode,
        suffix: backup_control::determine_backup_suffix(matches),
        owner_id,
        group_id,
        verbose: matches.get_flag(OPT_VERBOSE),
        preserve_timestamps,
        compare,
        strip,
        strip_program: String::from(
            matches
                .get_one::<String>(OPT_STRIP_PROGRAM)
                .map_or(DEFAULT_STRIP_PROGRAM, |s| s.as_str()),
        ),
        create_leading: matches.get_flag(OPT_CREATE_LEADING),
        target_dir,
        no_target_dir,
        preserve_context: matches.get_flag(OPT_PRESERVE_CONTEXT),
        context,
    })
}

/// Creates directories.
///
/// GNU man pages describe this functionality as creating 'all components of
/// the specified directories'.
///
/// Returns a Result type with the Err variant containing the error message.
///
fn directory(paths: &[String], b: &Behavior) -> UResult<()> {
    if paths.is_empty() {
        Err(InstallError::DirNeedsArg.into())
    } else {
        for path in paths.iter().map(Path::new) {
            // if the path already exist, don't try to create it again
            if !path.exists() {
                // Special case to match GNU's behavior:
                // install -d foo/. should work and just create foo/
                // std::fs::create_dir("foo/."); fails in pure Rust
                // See also mkdir.rs for another occurrence of this
                let path_to_create = dir_strip_dot_for_creation(path);
                // Differently than the primary functionality
                // (MainFunction::Standard), the directory functionality should
                // create all ancestors (or components) of a directory
                // regardless of the presence of the "-D" flag.
                //
                // NOTE: the GNU "install" sets the expected mode only for the
                // target directory. All created ancestor directories will have
                // the default mode. Hence it is safe to use fs::create_dir_all
                // and then only modify the target's dir mode.
                if let Err(e) = fs::create_dir_all(path_to_create.as_path())
                    .map_err_context(|| path_to_create.as_path().maybe_quote().to_string())
                {
                    show!(e);
                    continue;
                }

                if b.verbose {
                    println!(
                        "{}",
                        translate!("install-verbose-creating-directory", "path" => path_to_create.quote())
                    );
                }
            }

            if mode::chmod(path, b.mode()).is_err() {
                // Error messages are printed by the mode::chmod function!
                uucore::error::set_exit_code(1);
                continue;
            }

            show_if_err!(chown_optional_user_group(path, b));

            // Set SELinux context for directory if needed
            #[cfg(feature = "selinux")]
            show_if_err!(set_selinux_context(path, b));
        }
        // If the exit code was set, or show! has been called at least once
        // (which sets the exit code as well), function execution will end after
        // this return.
        Ok(())
    }
}

/// Test if the path is a new file path that can be
/// created immediately
fn is_new_file_path(path: &Path) -> bool {
    !path.exists()
        && (path.parent().is_none_or(Path::is_dir) || path.parent().unwrap().as_os_str().is_empty()) // In case of a simple file
}

/// Test if the path is an existing directory or ends with a trailing separator.
///
/// Returns true, if one of the conditions above is met; else false.
///
#[cfg(unix)]
fn is_potential_directory_path(path: &Path) -> bool {
    let separator = MAIN_SEPARATOR as u8;
    path.as_os_str().as_bytes().last() == Some(&separator) || path.is_dir()
}

#[cfg(not(unix))]
fn is_potential_directory_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.ends_with(MAIN_SEPARATOR) || path_str.ends_with('/') || path.is_dir()
}

/// Perform an install, given a list of paths and behavior.
///
/// Returns a Result type with the Err variant containing the error message.
///
#[allow(clippy::cognitive_complexity)]
fn standard(mut paths: Vec<String>, b: &Behavior) -> UResult<()> {
    // first check that paths contains at least one element
    if paths.is_empty() {
        return Err(UUsageError::new(
            1,
            translate!("install-error-missing-file-operand"),
        ));
    }
    if b.no_target_dir && paths.len() > 2 {
        return Err(InstallError::ExtraOperand(
            paths[2].clone(),
            format_usage(&translate!("install-usage")),
        )
        .into());
    }

    // get the target from either "-t foo" param or from the last given paths argument
    let target: PathBuf = if let Some(path) = &b.target_dir {
        path.into()
    } else {
        let last_path: PathBuf = paths.pop().unwrap().into();

        // paths has to contain more elements
        if paths.is_empty() {
            return Err(UUsageError::new(
                1,
                translate!("install-error-missing-destination-operand", "path" => last_path.to_str().unwrap()),
            ));
        }

        last_path
    };

    let sources = &paths.iter().map(PathBuf::from).collect::<Vec<_>>();

    if b.create_leading {
        // if -t is used in combination with -D, create whole target because it does not include filename
        let to_create: Option<&Path> = if b.target_dir.is_some() {
            Some(target.as_path())
        // if source and target are filenames used in combination with -D, create target's parent
        } else if !(sources.len() > 1 || is_potential_directory_path(&target)) {
            target.parent()
        } else {
            None
        };

        if let Some(to_create) = to_create {
            // if the path ends in /, remove it
            let to_create = if to_create.to_string_lossy().ends_with('/') {
                Path::new(to_create.to_str().unwrap().trim_end_matches('/'))
            } else {
                to_create
            };

            if !to_create.exists() {
                if b.verbose {
                    let mut result = PathBuf::new();
                    // When creating directories with -Dv, show directory creations step by step
                    for part in to_create.components() {
                        result.push(part.as_os_str());
                        if !result.is_dir() {
                            // Don't display when the directory already exists
                            println!(
                                "{}",
                                translate!("install-verbose-creating-directory-step", "path" => result.quote())
                            );
                        }
                    }
                }

                if let Err(e) = fs::create_dir_all(to_create) {
                    return Err(InstallError::CreateDirFailed(to_create.to_path_buf(), e).into());
                }
            }
        }
        if b.target_dir.is_some() {
            let p = to_create.unwrap();

            if !p.exists() || !p.is_dir() {
                return Err(InstallError::NotADirectory(p.to_path_buf()).into());
            }
        }
    }

    if sources.len() > 1 {
        copy_files_into_dir(sources, &target, b)
    } else {
        let source = sources.first().unwrap();

        if source.is_dir() {
            return Err(InstallError::OmittingDirectory(source.clone()).into());
        }

        if b.no_target_dir && target.is_dir() {
            return Err(
                InstallError::OverrideDirectoryFailed(target.clone(), source.clone()).into(),
            );
        }

        if is_potential_directory_path(&target) {
            return copy_files_into_dir(sources, &target, b);
        }

        if target.is_file() || is_new_file_path(&target) {
            copy(source, &target, b)
        } else {
            Err(InstallError::InvalidTarget(target).into())
        }
    }
}

/// Copy some files into a directory.
///
/// Prints verbose information and error messages.
/// Returns a Result type with the Err variant containing the error message.
///
/// # Parameters
///
/// `files` must all exist as non-directories.
/// `target_dir` must be a directory.
///
fn copy_files_into_dir(files: &[PathBuf], target_dir: &Path, b: &Behavior) -> UResult<()> {
    if !target_dir.is_dir() {
        return Err(InstallError::TargetDirIsntDir(target_dir.to_path_buf()).into());
    }
    for sourcepath in files {
        if let Err(err) = sourcepath
            .metadata()
            .map_err_context(|| format!("cannot stat {}", sourcepath.quote()))
        {
            show!(err);
            continue;
        }

        if sourcepath.is_dir() {
            let err = InstallError::OmittingDirectory(sourcepath.clone());
            show!(err);
            continue;
        }

        let mut targetpath = target_dir.to_path_buf();
        let filename = sourcepath.components().next_back().unwrap();
        targetpath.push(filename);

        show_if_err!(copy(sourcepath, &targetpath, b));
    }
    // If the exit code was set, or show! has been called at least once
    // (which sets the exit code as well), function execution will end after
    // this return.
    Ok(())
}

/// Handle incomplete user/group parings for chown.
///
/// Returns a Result type with the Err variant containing the error message.
/// If the user is root, revert the uid & gid
///
/// # Parameters
///
/// _path_ must exist.
///
/// # Errors
///
/// If the owner or group are invalid or copy system call fails, we print a verbose error and
/// return an empty error value.
///
fn chown_optional_user_group(path: &Path, b: &Behavior) -> UResult<()> {
    // GNU coreutils doesn't print chown operations during install with verbose flag.
    let verbosity = Verbosity {
        groups_only: b.owner_id.is_none(),
        level: VerbosityLevel::Normal,
    };

    // Determine the owner and group IDs to be used for chown.
    let (owner_id, group_id) = if b.owner_id.is_some() || b.group_id.is_some() {
        (b.owner_id, b.group_id)
    } else if geteuid() == 0 {
        // Special case for root user.
        (Some(0), Some(0))
    } else {
        // No chown operation needed.
        return Ok(());
    };

    let meta = match metadata(path) {
        Ok(meta) => meta,
        Err(e) => return Err(InstallError::MetadataFailed(e).into()),
    };
    match wrap_chown(path, &meta, owner_id, group_id, false, verbosity) {
        Ok(msg) if b.verbose && !msg.is_empty() => println!("chown: {msg}"),
        Ok(_) => {}
        Err(e) => return Err(InstallError::ChownFailed(path.to_path_buf(), e).into()),
    }

    Ok(())
}

/// Perform backup before overwriting.
///
/// # Parameters
///
/// * `to` - The destination file path.
/// * `b` - The behavior configuration.
///
/// # Returns
///
/// Returns an Option containing the backup path, or None if backup is not needed.
///
fn perform_backup(to: &Path, b: &Behavior) -> UResult<Option<PathBuf>> {
    if to.exists() {
        if b.verbose {
            println!(
                "{}",
                translate!("install-verbose-removed", "path" => to.quote())
            );
        }
        let backup_path = backup_control::get_backup_path(b.backup_mode, to, &b.suffix);
        if let Some(ref backup_path) = backup_path {
            fs::rename(to, backup_path).map_err(|err| {
                InstallError::BackupFailed(to.to_path_buf(), backup_path.clone(), err)
            })?;
        }
        Ok(backup_path)
    } else {
        Ok(None)
    }
}

/// Copy a non-special file using [`fs::copy`].
///
/// # Parameters
/// * `from` - The source file path.
/// * `to` - The destination file path.
///
/// # Returns
///
/// Returns an empty Result or an error in case of failure.
fn copy_normal_file(from: &Path, to: &Path) -> UResult<()> {
    if let Err(err) = fs::copy(from, to) {
        return Err(InstallError::InstallFailed(from.to_path_buf(), to.to_path_buf(), err).into());
    }
    Ok(())
}

/// Copy a file from one path to another. Handles the certain cases of special
/// files (e.g character specials).
///
/// # Parameters
///
/// * `from` - The source file path.
/// * `to` - The destination file path.
///
/// # Returns
///
/// Returns an empty Result or an error in case of failure.
///
fn copy_file(from: &Path, to: &Path) -> UResult<()> {
    if let Ok(to_abs) = to.canonicalize() {
        if from.canonicalize()? == to_abs {
            return Err(InstallError::SameFile(from.to_path_buf(), to.to_path_buf()).into());
        }
    }

    if to.is_dir() && !from.is_dir() {
        return Err(InstallError::OverrideDirectoryFailed(
            to.to_path_buf().clone(),
            from.to_path_buf().clone(),
        )
        .into());
    }
    // fs::copy fails if destination is a invalid symlink.
    // so lets just remove all existing files at destination before copy.
    if let Err(e) = fs::remove_file(to) {
        if e.kind() != std::io::ErrorKind::NotFound {
            show_error!(
                "{}",
                translate!("install-error-failed-to-remove", "path" => to.display(), "error" => format!("{e:?}"))
            );
        }
    }

    let ft = match metadata(from) {
        Ok(ft) => ft.file_type(),
        Err(err) => {
            return Err(
                InstallError::InstallFailed(from.to_path_buf(), to.to_path_buf(), err).into(),
            );
        }
    };

    // Stream-based copying to get around the limitations of std::fs::copy
    #[cfg(unix)]
    if ft.is_char_device() || ft.is_block_device() || ft.is_fifo() {
        let mut handle = File::open(from)?;
        let mut dest = File::create(to)?;
        copy_stream(&mut handle, &mut dest)?;
        return Ok(());
    }

    copy_normal_file(from, to)?;

    Ok(())
}

/// Strip a file using an external program.
///
/// # Parameters
///
/// * `to` - The destination file path.
/// * `b` - The behavior configuration.
///
/// # Returns
///
/// Returns an empty Result or an error in case of failure.
///
fn strip_file(to: &Path, b: &Behavior) -> UResult<()> {
    // Check if the filename starts with a hyphen and adjust the path
    let to_str = to.as_os_str().to_str().unwrap_or_default();
    let to = if to_str.starts_with('-') {
        let mut new_path = PathBuf::from(".");
        new_path.push(to);
        new_path
    } else {
        to.to_path_buf()
    };
    match process::Command::new(&b.strip_program).arg(&to).status() {
        Ok(status) => {
            if !status.success() {
                // Follow GNU's behavior: if strip fails, removes the target
                let _ = fs::remove_file(to);
                return Err(InstallError::StripProgramFailed(
                    translate!("install-error-strip-abnormal", "code" => status.code().unwrap()),
                )
                .into());
            }
        }
        Err(e) => {
            // Follow GNU's behavior: if strip fails, removes the target
            let _ = fs::remove_file(to);
            return Err(InstallError::StripProgramFailed(e.to_string()).into());
        }
    }
    Ok(())
}

/// Set ownership and permissions on the destination file.
///
/// # Parameters
///
/// * `to` - The destination file path.
/// * `b` - The behavior configuration.
///
/// # Returns
///
/// Returns an empty Result or an error in case of failure.
///
fn set_ownership_and_permissions(to: &Path, b: &Behavior) -> UResult<()> {
    // Silent the warning as we want to the error message
    #[allow(clippy::question_mark)]
    if mode::chmod(to, b.mode()).is_err() {
        return Err(InstallError::ChmodFailed(to.to_path_buf()).into());
    }

    chown_optional_user_group(to, b)?;

    Ok(())
}

/// Preserve timestamps on the destination file.
///
/// # Parameters
///
/// * `from` - The source file path.
/// * `to` - The destination file path.
///
/// # Returns
///
/// Returns an empty Result or an error in case of failure.
///
fn preserve_timestamps(from: &Path, to: &Path) -> UResult<()> {
    let meta = match metadata(from) {
        Ok(meta) => meta,
        Err(e) => return Err(InstallError::MetadataFailed(e).into()),
    };

    let modified_time = FileTime::from_last_modification_time(&meta);
    let accessed_time = FileTime::from_last_access_time(&meta);

    if let Err(e) = set_file_times(to, accessed_time, modified_time) {
        show_error!("{e}");
        // ignore error
    }
    Ok(())
}

/// Copy one file to a new location, changing metadata.
///
/// Returns a Result type with the Err variant containing the error message.
///
/// # Parameters
///
/// _from_ must exist as a non-directory.
/// _to_ must be a non-existent file, whose parent directory exists.
///
/// # Errors
///
/// If the copy system call fails, we print a verbose error and return an empty error value.
///
fn copy(from: &Path, to: &Path, b: &Behavior) -> UResult<()> {
    if b.compare && !need_copy(from, to, b) {
        return Ok(());
    }
    // Declare the path here as we may need it for the verbose output below.
    let backup_path = perform_backup(to, b)?;

    copy_file(from, to)?;

    #[cfg(not(windows))]
    if b.strip {
        strip_file(to, b)?;
    }

    set_ownership_and_permissions(to, b)?;

    if b.preserve_timestamps {
        preserve_timestamps(from, to)?;
    }

    #[cfg(feature = "selinux")]
    if b.preserve_context {
        uucore::selinux::preserve_security_context(from, to)
            .map_err(|e| InstallError::SelinuxContextFailed(e.to_string()))?;
    } else if b.context.is_some() {
        set_selinux_context(to, b)?;
    }

    if b.verbose {
        print!(
            "{}",
            translate!("install-verbose-copy", "from" => from.quote(), "to" => to.quote())
        );
        match backup_path {
            Some(path) => println!(
                " {}",
                translate!("install-verbose-backup", "backup" => path.quote())
            ),
            None => println!(),
        }
    }

    Ok(())
}

/// Check if a file needs to be copied due to ownership differences when no explicit group is specified.
/// Returns true if the destination file's ownership would differ from what it should be after installation.
fn needs_copy_for_ownership(to: &Path, to_meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    // Check if the destination file's owner differs from the effective user ID
    if to_meta.uid() != geteuid() {
        return true;
    }

    // For group, we need to determine what the group would be after installation
    // If no group is specified, the behavior depends on the directory:
    // - If the directory has setgid bit, the file inherits the directory's group
    // - Otherwise, the file gets the user's effective group
    let expected_gid = to
        .parent()
        .and_then(|parent| metadata(parent).ok())
        .filter(|parent_meta| parent_meta.mode() & 0o2000 != 0)
        .map_or(getegid(), |parent_meta| parent_meta.gid());

    to_meta.gid() != expected_gid
}

/// Return true if a file is necessary to copy. This is the case when:
///
/// - _from_ or _to_ is nonexistent;
/// - either file has a sticky bit or set\[ug\]id bit, or the user specified one;
/// - either file isn't a regular file;
/// - the sizes of _from_ and _to_ differ;
/// - _to_'s owner differs from intended; or
/// - the contents of _from_ and _to_ differ.
///
/// # Parameters
///
/// _from_ and _to_, if existent, must be non-directories.
///
/// # Errors
///
/// Crashes the program if a nonexistent owner or group is specified in _b_.
///
fn need_copy(from: &Path, to: &Path, b: &Behavior) -> bool {
    // Attempt to retrieve metadata for the source file.
    // If this fails, assume the file needs to be copied.
    let Ok(from_meta) = metadata(from) else {
        return true;
    };

    // Attempt to retrieve metadata for the destination file.
    // If this fails, assume the file needs to be copied.
    let Ok(to_meta) = metadata(to) else {
        return true;
    };

    // Check if the destination is a symlink (should always be replaced)
    if let Ok(to_symlink_meta) = fs::symlink_metadata(to) {
        if to_symlink_meta.file_type().is_symlink() {
            return true;
        }
    }

    // Define special file mode bits (setuid, setgid, sticky).
    let extra_mode: u32 = 0o7000;
    // Define all file mode bits (including permissions).
    // setuid || setgid || sticky || permissions
    let all_modes: u32 = 0o7777;

    // Check if any special mode bits are set in the specified mode,
    // source file mode, or destination file mode.
    if b.mode() & extra_mode != 0
        || from_meta.mode() & extra_mode != 0
        || to_meta.mode() & extra_mode != 0
    {
        return true;
    }

    // Check if the mode of the destination file differs from the specified mode.
    if b.mode() != to_meta.mode() & all_modes {
        return true;
    }

    // Check if either the source or destination is not a file.
    if !from_meta.is_file() || !to_meta.is_file() {
        return true;
    }

    // Check if the file sizes differ.
    if from_meta.len() != to_meta.len() {
        return true;
    }

    #[cfg(feature = "selinux")]
    if b.preserve_context && contexts_differ(from, to) {
        return true;
    }

    // TODO: if -P (#1809) and from/to contexts mismatch, return true.

    // Check if the owner ID is specified and differs from the destination file's owner.
    if let Some(owner_id) = b.owner_id {
        if owner_id != to_meta.uid() {
            return true;
        }
    }

    // Check if the group ID is specified and differs from the destination file's group.
    if let Some(group_id) = b.group_id {
        if group_id != to_meta.gid() {
            return true;
        }
    } else if needs_copy_for_ownership(to, &to_meta) {
        return true;
    }

    // Check if the contents of the source and destination files differ.
    if !diff(from.to_str().unwrap(), to.to_str().unwrap()) {
        return true;
    }

    false
}

#[cfg(feature = "selinux")]
fn set_selinux_context(path: &Path, behavior: &Behavior) -> UResult<()> {
    if !behavior.preserve_context && behavior.context.is_some() {
        // Use the provided context set by -Z/--context
        set_selinux_security_context(path, behavior.context.as_ref())
            .map_err(|e| InstallError::SelinuxContextFailed(e.to_string()))?;
    }
    Ok(())
}
