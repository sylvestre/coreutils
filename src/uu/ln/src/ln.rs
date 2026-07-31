// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) srcpath targetpath EEXIST dirfd

use clap::{Arg, ArgAction, Command};
use std::io::{self, Write, stdout};
use uucore::display::Quotable;
use uucore::error::{UError, UIoError, UResult};

use uucore::fs::{make_path_relative_to, paths_refer_to_same_file};
use uucore::translate;
use uucore::{format_usage, prompt_yes, show_error};

use std::borrow::Cow;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use thiserror::Error;

#[cfg(any(unix, target_os = "redox"))]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};
use std::path::{Path, PathBuf};
use uucore::backup_control::{self, BackupMode};
use uucore::fs::{MissingHandling, ResolveMode, canonicalize};
#[cfg(unix)]
use uucore::safe_traversal::{DirFd, SymlinkBehavior};

/// Public visibility allows other apps to integrate with our
/// `ln` utility by calling `exec` directly with their `Settings`.
pub struct Settings {
    pub overwrite: OverwriteMode,
    pub backup: BackupMode,
    pub suffix: OsString,
    pub symbolic: bool,
    pub relative: bool,
    pub logical: bool,
    pub target_dir: Option<PathBuf>,
    pub no_target_dir: bool,
    pub no_dereference: bool,
    pub verbose: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverwriteMode {
    NoClobber,
    Interactive,
    Force,
}

#[derive(Error, Debug)]
pub enum LnError {
    #[error("{}", translate!("ln-error-target-is-not-directory", "target" => _0.quote()))]
    TargetIsNotADirectory(PathBuf),

    #[error("{0}")]
    Io(#[from] UIoError),

    #[error("{1}: {0}")]
    IoContext(UIoError, String),

    #[error("")]
    SomeLinksFailed,

    #[error("{}", translate!("ln-error-same-file", "file1" => _0.quote(), "file2" => _1.quote()))]
    SameFile(PathBuf, PathBuf),

    #[error("{}", translate!("ln-error-missing-destination", "operand" => _0.quote()))]
    MissingDestination(PathBuf),

    #[error("{}", translate!("ln-error-extra-operand", "operand" => _0.quote(), "program" => _1.clone()))]
    ExtraOperand(OsString, String),

    #[error("{}", translate!("ln-failed-to-create-hard-link-dir", "source" => _0.to_string_lossy()))]
    FailedToCreateHardLinkDir(PathBuf),
}

impl UError for LnError {
    fn code(&self) -> i32 {
        1
    }
}
pub type LnResult<T> = Result<T, LnError>;

impl From<io::Error> for LnError {
    fn from(err: io::Error) -> Self {
        Self::Io(UIoError::from(err))
    }
}

mod options {
    pub const FORCE: &str = "force";
    //pub const DIRECTORY: &str = "directory";
    pub const INTERACTIVE: &str = "interactive";
    pub const NO_DEREFERENCE: &str = "no-dereference";
    pub const SYMBOLIC: &str = "symbolic";
    pub const LOGICAL: &str = "logical";
    pub const PHYSICAL: &str = "physical";
    pub const TARGET_DIRECTORY: &str = "target-directory";
    pub const NO_TARGET_DIRECTORY: &str = "no-target-directory";
    pub const RELATIVE: &str = "relative";
    pub const VERBOSE: &str = "verbose";
}

static ARG_FILES: &str = "files";

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uucore::clap_localization::handle_clap_result(uu_app(), args)?;

    /* the list of files */

    let paths: Vec<PathBuf> = matches
        .get_many::<OsString>(ARG_FILES)
        .unwrap()
        .map(PathBuf::from)
        .collect();

    let symbolic = matches.get_flag(options::SYMBOLIC);

    let overwrite_mode = if matches.get_flag(options::FORCE) {
        OverwriteMode::Force
    } else if matches.get_flag(options::INTERACTIVE) {
        OverwriteMode::Interactive
    } else {
        OverwriteMode::NoClobber
    };

    let backup_mode =
        backup_control::determine_backup_mode(std::env::var("VERSION_CONTROL").ok(), &matches)?;
    let backup_suffix = backup_control::determine_backup_suffix(&matches);

    // When we have "-L" or "-L -P", false otherwise
    let logical = matches.get_flag(options::LOGICAL);

    let settings = Settings {
        overwrite: overwrite_mode,
        backup: backup_mode,
        suffix: OsString::from(backup_suffix),
        symbolic,
        logical,
        relative: matches.get_flag(options::RELATIVE),
        target_dir: matches
            .get_one::<OsString>(options::TARGET_DIRECTORY)
            .map(PathBuf::from),
        no_target_dir: matches.get_flag(options::NO_TARGET_DIRECTORY),
        no_dereference: matches.get_flag(options::NO_DEREFERENCE),
        verbose: matches.get_flag(options::VERBOSE),
    };

    exec(&paths[..], &settings)?;
    Ok(())
}

pub fn uu_app() -> Command {
    let after_help = format!(
        "{}\n\n{}",
        translate!("ln-after-help"),
        backup_control::BACKUP_CONTROL_LONG_HELP
    );

    Command::new("ln")
        .version(uucore::crate_version!())
        .help_template(uucore::localized_help_template("ln"))
        .about(translate!("ln-about"))
        .override_usage(format_usage(&translate!("ln-usage")))
        .infer_long_args(true)
        .after_help(after_help)
        .arg(backup_control::arguments::backup())
        .arg(backup_control::arguments::backup_no_args())
        /*.arg(
            Arg::new(options::DIRECTORY)
                .short('d')
                .long(options::DIRECTORY)
                .help("allow users with appropriate privileges to attempt to make hard links to directories")
        )*/
        .arg(
            Arg::new(options::FORCE)
                .short('f')
                .long(options::FORCE)
                .help(translate!("ln-help-force"))
                .overrides_with(options::INTERACTIVE)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::INTERACTIVE)
                .short('i')
                .long(options::INTERACTIVE)
                .help(translate!("ln-help-interactive"))
                .overrides_with(options::FORCE)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NO_DEREFERENCE)
                .short('n')
                .long(options::NO_DEREFERENCE)
                .help(translate!("ln-help-no-dereference"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::LOGICAL)
                .short('L')
                .long(options::LOGICAL)
                .help(translate!("ln-help-logical"))
                .overrides_with(options::PHYSICAL)
                .action(ArgAction::SetTrue),
        )
        .arg(
            // Not implemented yet
            Arg::new(options::PHYSICAL)
                .short('P')
                .long(options::PHYSICAL)
                .help(translate!("ln-help-physical"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::SYMBOLIC)
                .short('s')
                .long(options::SYMBOLIC)
                .help(translate!("ln-help-symbolic"))
                // override added for https://github.com/uutils/coreutils/issues/2359
                .overrides_with(options::SYMBOLIC)
                .action(ArgAction::SetTrue),
        )
        .arg(backup_control::arguments::suffix())
        .arg(
            Arg::new(options::TARGET_DIRECTORY)
                .short('t')
                .long(options::TARGET_DIRECTORY)
                .help(translate!("ln-help-target-directory"))
                .value_name("DIRECTORY")
                .value_hint(clap::ValueHint::DirPath)
                .value_parser(clap::value_parser!(OsString))
                .conflicts_with(options::NO_TARGET_DIRECTORY),
        )
        .arg(
            Arg::new(options::NO_TARGET_DIRECTORY)
                .short('T')
                .long(options::NO_TARGET_DIRECTORY)
                .help(translate!("ln-help-no-target-directory"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::RELATIVE)
                .short('r')
                .long(options::RELATIVE)
                .help(translate!("ln-help-relative"))
                .requires(options::SYMBOLIC)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::VERBOSE)
                .short('v')
                .long(options::VERBOSE)
                .help(translate!("ln-help-verbose"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(ARG_FILES)
                .action(ArgAction::Append)
                .value_hint(clap::ValueHint::AnyPath)
                .value_parser(clap::value_parser!(OsString))
                .required(true)
                .num_args(1..),
        )
}

/// Executes the `ln` utility with the given paths and settings.
///
/// This is made public to allow other apps to use `ln` as a library.
pub fn exec(files: &[PathBuf], settings: &Settings) -> LnResult<()> {
    // Handle cases where we create links in a directory first.
    if let Some(ref target_path) = settings.target_dir {
        // 4th form: a directory is specified by -t.
        return link_files_in_dir(files, target_path, settings);
    }
    if !settings.no_target_dir {
        if files.len() == 1 {
            // 2nd form: the target directory is the current directory.
            return link_files_in_dir(files, &PathBuf::from("."), settings);
        }
        let last_file = &PathBuf::from(files.last().unwrap());
        if files.len() > 2 || last_file.is_dir() {
            // 3rd form: create links in the last argument.
            return link_files_in_dir(&files[0..files.len() - 1], last_file, settings);
        }
    }

    // 1st form. Now there should be only two operands, but if -T is
    // specified we may have a wrong number of operands.
    if files.len() == 1 {
        return Err(LnError::MissingDestination(files[0].clone()));
    }
    if files.len() > 2 {
        return Err(LnError::ExtraOperand(
            files[2].clone().into(),
            uucore::execution_phrase().to_string(),
        ));
    }
    assert!(!files.is_empty());

    link(&files[0], &files[1], settings)
}

/// The directory a link is being created in, held open so that replacing the
/// directory after the `is_dir` check cannot redirect the link creation.
///
/// GNU opens NEWDIR once (`openat_safer`) and creates every link with
/// `symlinkat`/`linkat` relative to that descriptor; only the two-operand form
/// uses `AT_FDCWD`. This mirrors that. On non-unix targets there is no
/// descriptor and every operation stays path-based, as before.
#[cfg(unix)]
type Anchor<'a> = Option<(&'a DirFd, &'a OsStr)>;
#[cfg(not(unix))]
type Anchor<'a> = Option<(&'a (), &'a OsStr)>;

#[allow(clippy::cognitive_complexity)]
fn link_files_in_dir(files: &[PathBuf], target_dir: &Path, settings: &Settings) -> LnResult<()> {
    if !target_dir.is_dir() {
        return Err(LnError::TargetIsNotADirectory(target_dir.to_owned()));
    }

    // Opened once, before any link is created. A failure here is not fatal:
    // the path-based fallback below still reports whatever the real error is,
    // and keeps behaviour identical on platforms without openat.
    #[cfg(unix)]
    let dir_fd = DirFd::open(target_dir, SymlinkBehavior::Follow).ok();
    // remember the linked destinations for further usage
    let mut linked_destinations: HashSet<PathBuf> = HashSet::with_capacity(files.len());

    let mut all_successful = true;
    for srcpath in files {
        let targetpath = if settings.no_dereference && target_dir.is_symlink() {
            let remove_target = || {
                // Not sure why but on Windows, the symlink can be
                // considered as a dir
                // See test_ln::test_symlink_no_deref_dir
                #[cfg(windows)]
                if let Err(e) = fs::remove_dir(target_dir) {
                    show_error!(
                        "{}",
                        translate!("ln-error-could-not-update", "target" => target_dir.quote(), "error" => e)
                    );
                }
            };
            match settings.overwrite {
                OverwriteMode::NoClobber => {}
                OverwriteMode::Interactive => {
                    if prompt_yes!(
                        "{}",
                        translate!("ln-prompt-replace", "file" => target_dir.quote())
                    ) {
                        remove_target();
                    }
                }
                OverwriteMode::Force => {
                    remove_target();
                }
            }
            target_dir.to_path_buf()
        } else {
            match srcpath.file_name() {
                Some(basename) => target_dir.join(basename),
                // This can be None only for "." or "..". Trying
                // to create a link with such name will fail with
                // EEXIST, which agrees with the behavior of GNU
                // coreutils.
                None => target_dir.join(srcpath),
            }
        };

        // The --no-dereference branch above links over target_dir itself rather
        // than into it, so it is not a name inside the anchored directory.
        let target_is_dir_itself = targetpath == target_dir;
        #[cfg(unix)]
        let anchor: Anchor = match (&dir_fd, targetpath.file_name()) {
            (Some(fd), Some(base)) if !target_is_dir_itself => Some((fd, base)),
            _ => None,
        };
        #[cfg(not(unix))]
        let anchor: Anchor = None;

        if linked_destinations.contains(&targetpath) {
            // If the target file was already created in this ln call, do not overwrite
            show_error!(
                "{}",
                translate!("ln-error-will-not-overwrite", "target" => targetpath.quote(), "source" => srcpath.quote())
            );
            all_successful = false;
        } else if let Err(e) = link_impl(srcpath, &targetpath, settings, anchor) {
            show_error!("{e}");
            all_successful = false;
        }

        linked_destinations.insert(targetpath.clone());
    }
    if all_successful {
        Ok(())
    } else {
        Err(LnError::SomeLinksFailed)
    }
}

fn relative_path<'a>(src: &'a Path, dst: &Path) -> Cow<'a, Path> {
    // `dst.parent()` is None for a destination with no parent (`/`, `""`, or a
    // bare Windows prefix). Fall through to the non-relative `src` rather than
    // unwrapping it; the caller then reports the usual error.
    let Some(dst_parent) = dst.parent() else {
        return src.into();
    };
    let (Ok(src_abs), Ok(dst_abs)) = (
        canonicalize(src, MissingHandling::Missing, ResolveMode::Physical),
        canonicalize(dst_parent, MissingHandling::Missing, ResolveMode::Physical),
    ) else {
        return src.into();
    };

    make_path_relative_to(src_abs, dst_abs).into()
}

/// Decide whether `src` and `dst` are actually the same directory entry.
fn is_same_entry(src: &Path, dst: &Path) -> bool {
    match (
        canonicalize(src, MissingHandling::Missing, ResolveMode::Physical),
        canonicalize(dst, MissingHandling::Missing, ResolveMode::Physical),
    ) {
        (Ok(src), Ok(dst)) => src == dst,
        _ => true,
    }
}

fn link(src: &Path, dst: &Path, settings: &Settings) -> LnResult<()> {
    link_impl(src, dst, settings, None)
}

/// Remove the destination, through the anchored directory when there is one.
fn remove_dest(dst: &Path, anchor: Anchor) -> io::Result<()> {
    match anchor {
        #[cfg(unix)]
        Some((fd, base)) => fd.unlink_at(base, false),
        _ => fs::remove_file(dst),
    }
}

#[allow(clippy::cognitive_complexity)]
fn link_impl(src: &Path, dst: &Path, settings: &Settings, anchor: Anchor) -> LnResult<()> {
    let mut backup_path = None;
    let source: Cow<'_, Path> = if settings.relative {
        relative_path(src, dst)
    } else {
        src.into()
    };

    if dst.is_symlink() || dst.exists() {
        backup_path = backup_control::get_backup_path(settings.backup, dst, &settings.suffix);
        if settings.backup == BackupMode::Existing && !settings.symbolic {
            // when ln --backup f f, it should detect that it is the same file
            if paths_refer_to_same_file(src, dst, true) && is_same_entry(src, dst) {
                return Err(LnError::SameFile(src.to_owned(), dst.to_owned()));
            }
        }
        if let Some(ref p) = backup_path {
            let renamed = match (anchor, p.file_name()) {
                #[cfg(unix)]
                (Some((fd, base)), Some(backup_base)) => fd.rename_at(base, backup_base),
                _ => fs::rename(dst, p),
            };
            renamed.map_err(|e| {
                LnError::IoContext(
                    UIoError::from(e),
                    translate!("ln-cannot-backup", "file" => dst.quote()),
                )
            })?;
        }
        match settings.overwrite {
            OverwriteMode::NoClobber => {}
            OverwriteMode::Interactive => {
                if !prompt_yes!("{}", translate!("ln-prompt-replace", "file" => dst.quote())) {
                    return Err(LnError::SomeLinksFailed);
                }

                let _ = remove_dest(dst, anchor);
                // In case of error, don't do anything
            }
            OverwriteMode::Force => {
                if !dst.is_symlink()
                    && paths_refer_to_same_file(src, dst, true)
                    && is_same_entry(src, dst)
                {
                    // Even in force overwrite mode, verify we are not targeting the same entry and return a SameFile error if so
                    return Err(LnError::SameFile(src.to_owned(), dst.to_owned()));
                }
                let _ = remove_dest(dst, anchor);
                // In case of error, don't do anything
            }
        }
    }

    let res = if settings.symbolic {
        let created = match anchor {
            #[cfg(unix)]
            Some((fd, base)) => fd.symlink_at(&source, base),
            _ => symlink(&source, dst),
        };
        created.map_err(|e| {
            LnError::IoContext(
                UIoError::from(e),
                translate!(
                    "ln-failed-to-create-symbolic-link",
                    "dest" => dst.quote()
                ),
            )
        })
    } else {
        let p = if settings.logical && source.is_symlink() {
            fs::canonicalize(&source).map_err(|e| {
                LnError::IoContext(
                    UIoError::from(e),
                    translate!("ln-failed-to-access", "file" => source.quote()),
                )
            })?
        } else {
            source.to_path_buf()
        };
        let created = match anchor {
            #[cfg(unix)]
            Some((fd, base)) => fd.link_at(&p, base),
            _ => fs::hard_link(&p, dst),
        };
        match created {
            Ok(()) => Ok(()),
            Err(_) if p.is_dir() => Err(LnError::FailedToCreateHardLinkDir(source.to_path_buf())),
            Err(e) => Err(LnError::IoContext(
                UIoError::from(e),
                translate!(
                    "ln-failed-to-create-hard-link",
                    "source" => source.quote(),
                    "dest" => dst.quote()
                ),
            )),
        }
    };

    if let Err(e) = res {
        if let Some(ref p) = backup_path {
            let restored = match (anchor, p.file_name()) {
                #[cfg(unix)]
                (Some((fd, base)), Some(backup_base)) => fd.rename_at(backup_base, base),
                _ => fs::rename(p, dst),
            };
            restored.map_err(|e| {
                LnError::IoContext(
                    UIoError::from(e),
                    translate!("ln-cannot-backup", "file" => dst.quote()),
                )
            })?;
        }
        return Err(e);
    }

    if settings.verbose {
        let mut out = stdout();
        write!(out, "{} -> {}", dst.quote(), source.quote())?;
        match backup_path {
            Some(path) => writeln!(
                out,
                " ({})",
                translate!("ln-backup", "backup" => path.quote())
            )?,
            None => writeln!(out)?,
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn symlink<P1: AsRef<Path>, P2: AsRef<Path>>(src: P1, dst: P2) -> io::Result<()> {
    if src.as_ref().is_dir() {
        symlink_dir(src, dst)
    } else {
        symlink_file(src, dst)
    }
}

#[cfg(target_os = "wasi")]
pub fn symlink<P1: AsRef<Path>, P2: AsRef<Path>>(src: P1, dst: P2) -> io::Result<()> {
    rustix::fs::symlink(src.as_ref(), dst.as_ref()).map_err(io::Error::from)
}
