// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (vars) RFILE RDONLY CLOEXEC NOFOLLOW ENODATA relabeler

#![cfg(any(target_os = "linux", target_os = "android"))]
#![allow(clippy::upper_case_acronyms)]

use clap::builder::ValueParser;
use uucore::error::{UResult, USimpleError, UUsageError};
use uucore::translate;
use uucore::{display::Quotable, format_usage, show_error, show_warning};

use clap::{Arg, ArgAction, ArgMatches, Command};
use rustix::fs::{FileType, Mode, OFlags, openat};
use selinux::{OpaqueSecurityContext, SecurityContext};

use uucore::safe_traversal::DirFd;

use core::ffi::CStr;
use std::borrow::Cow;
use std::ffi::{CString, OsStr, OsString};
use std::io::Write;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, IntoRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::{fs, io};

mod errors;

use errors::{Error, Result, report_full_error};

pub mod options {
    pub static HELP: &str = "help";
    pub static VERBOSE: &str = "verbose";

    pub static REFERENCE: &str = "reference";

    pub static USER: &str = "user";
    pub static ROLE: &str = "role";
    pub static TYPE: &str = "type";
    pub static RANGE: &str = "range";

    pub static RECURSIVE: &str = "recursive";

    pub mod sym_links {
        pub static FOLLOW_ARG_DIR_SYM_LINK: &str = "follow-arg-dir-sym-link";
        pub static FOLLOW_DIR_SYM_LINKS: &str = "follow-dir-sym-links";
        pub static NO_FOLLOW_SYM_LINKS: &str = "no-follow-sym-links";
    }

    pub mod dereference {
        pub static DEREFERENCE: &str = "dereference";
        pub static NO_DEREFERENCE: &str = "no-dereference";
    }

    pub mod preserve_root {
        pub static PRESERVE_ROOT: &str = "preserve-root";
        pub static NO_PRESERVE_ROOT: &str = "no-preserve-root";
    }
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uucore::clap_localization::handle_clap_result(uu_app(), args)?;

    let options = match parse_command_line(&matches) {
        Ok(r) => r,
        Err(r) => {
            if let Error::CommandLine(r) = r {
                return Err(r.into());
            }

            return Err(UUsageError::new(libc::EXIT_FAILURE, format!("{r}.\n")));
        }
    };

    let context = match &options.mode {
        CommandLineMode::ReferenceBased { reference } => {
            let result = match SecurityContext::of_path(reference, true, false) {
                Ok(Some(context)) => Ok(context),

                Ok(None) => {
                    let err = io::Error::from_raw_os_error(libc::ENODATA);
                    Err(Error::from_io1(
                        translate!("chcon-op-getting-security-context"),
                        reference,
                        err,
                    ))
                }

                Err(r) => Err(Error::from_selinux(
                    translate!("chcon-op-getting-security-context"),
                    r,
                )),
            };

            match result {
                Err(r) => {
                    return Err(USimpleError::new(
                        libc::EXIT_FAILURE,
                        format!("{}.", report_full_error(&r)),
                    ));
                }

                Ok(file_context) => SELinuxSecurityContext::File(file_context),
            }
        }

        CommandLineMode::ContextBased { context } => {
            let c_context = match os_str_to_c_string(context) {
                Ok(context) => context,

                Err(_r) => {
                    return Err(USimpleError::new(
                        libc::EXIT_FAILURE,
                        translate!("chcon-error-invalid-context", "context" => context.quote()),
                    ));
                }
            };

            if SecurityContext::from_c_str(&c_context, false).check() == Some(false) {
                return Err(USimpleError::new(
                    libc::EXIT_FAILURE,
                    translate!("chcon-error-invalid-context", "context" => context.quote()),
                ));
            }

            SELinuxSecurityContext::String(Some(c_context))
        }

        CommandLineMode::Custom { .. } => SELinuxSecurityContext::String(None),
    };

    let root_id = if options.preserve_root && options.recursive_mode.is_recursive() {
        match root_identity() {
            Ok(r) => Some(r),

            Err(r) => {
                return Err(USimpleError::new(
                    libc::EXIT_FAILURE,
                    format!("{}.", report_full_error(&r)),
                ));
            }
        }
    } else {
        None
    };

    match relabel_all(&options, &context, root_id) {
        Ok(true) => Ok(()),
        Ok(false) => Err(libc::EXIT_FAILURE.into()),
        Err(r) => Err(USimpleError::new(
            libc::EXIT_FAILURE,
            format!("{}.", report_full_error(&r)),
        )),
    }
}

pub fn uu_app() -> Command {
    let cmd = Command::new("chcon")
        .version(uucore::crate_version!())
        .about(translate!("chcon-about"))
        .override_usage(format_usage(&translate!("chcon-usage")))
        .infer_long_args(true);
    uucore::clap_localization::configure_localized_command(cmd)
        .args_override_self(true)
        .disable_help_flag(true)
        .arg(
            Arg::new(options::dereference::DEREFERENCE)
                .long(options::dereference::DEREFERENCE)
                .overrides_with(options::dereference::NO_DEREFERENCE)
                .help(translate!("chcon-help-dereference"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::dereference::NO_DEREFERENCE)
                .short('h')
                .long(options::dereference::NO_DEREFERENCE)
                .help(translate!("chcon-help-no-dereference"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("help")
                .long("help")
                .help(translate!("help"))
                .action(ArgAction::Help),
        )
        .arg(
            Arg::new(options::preserve_root::PRESERVE_ROOT)
                .long(options::preserve_root::PRESERVE_ROOT)
                .overrides_with(options::preserve_root::NO_PRESERVE_ROOT)
                .help(translate!("chcon-help-preserve-root"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::preserve_root::NO_PRESERVE_ROOT)
                .long(options::preserve_root::NO_PRESERVE_ROOT)
                .help(translate!("chcon-help-no-preserve-root"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::REFERENCE)
                .long(options::REFERENCE)
                .value_name("RFILE")
                .value_hint(clap::ValueHint::FilePath)
                .conflicts_with_all([options::USER, options::ROLE, options::TYPE, options::RANGE])
                .help(translate!("chcon-help-reference"))
                .value_parser(ValueParser::os_string()),
        )
        .arg(
            Arg::new(options::USER)
                .short('u')
                .long(options::USER)
                .value_name("USER")
                .value_hint(clap::ValueHint::Username)
                .help(translate!("chcon-help-user"))
                .value_parser(ValueParser::os_string()),
        )
        .arg(
            Arg::new(options::ROLE)
                .short('r')
                .long(options::ROLE)
                .value_name("ROLE")
                .help(translate!("chcon-help-role"))
                .value_parser(ValueParser::os_string()),
        )
        .arg(
            Arg::new(options::TYPE)
                .short('t')
                .long(options::TYPE)
                .value_name("TYPE")
                .help(translate!("chcon-help-type"))
                .value_parser(ValueParser::os_string()),
        )
        .arg(
            Arg::new(options::RANGE)
                .short('l')
                .long(options::RANGE)
                .value_name("RANGE")
                .help(translate!("chcon-help-range"))
                .value_parser(ValueParser::os_string()),
        )
        .arg(
            Arg::new(options::RECURSIVE)
                .short('R')
                .long(options::RECURSIVE)
                .help(translate!("chcon-help-recursive"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::sym_links::FOLLOW_ARG_DIR_SYM_LINK)
                .short('H')
                .requires(options::RECURSIVE)
                .overrides_with_all([
                    options::sym_links::FOLLOW_DIR_SYM_LINKS,
                    options::sym_links::NO_FOLLOW_SYM_LINKS,
                ])
                .help(translate!("chcon-help-follow-arg-dir-symlink"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::sym_links::FOLLOW_DIR_SYM_LINKS)
                .short('L')
                .requires(options::RECURSIVE)
                .overrides_with_all([
                    options::sym_links::FOLLOW_ARG_DIR_SYM_LINK,
                    options::sym_links::NO_FOLLOW_SYM_LINKS,
                ])
                .help(translate!("chcon-help-follow-dir-symlinks"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::sym_links::NO_FOLLOW_SYM_LINKS)
                .short('P')
                .requires(options::RECURSIVE)
                .overrides_with_all([
                    options::sym_links::FOLLOW_ARG_DIR_SYM_LINK,
                    options::sym_links::FOLLOW_DIR_SYM_LINKS,
                ])
                .help(translate!("chcon-help-no-follow-symlinks"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::VERBOSE)
                .short('v')
                .long(options::VERBOSE)
                .help(translate!("chcon-help-verbose"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("FILE")
                .action(ArgAction::Append)
                .value_hint(clap::ValueHint::FilePath)
                .num_args(1..)
                .value_parser(ValueParser::os_string()),
        )
}

#[derive(Debug)]
struct Options {
    verbose: bool,
    preserve_root: bool,
    recursive_mode: RecursiveMode,
    affect_symlink_referent: bool,
    mode: CommandLineMode,
    files: Vec<PathBuf>,
}

fn parse_command_line(matches: &ArgMatches) -> Result<Options> {
    let verbose = matches.get_flag(options::VERBOSE);

    let (recursive_mode, affect_symlink_referent) = if matches.get_flag(options::RECURSIVE) {
        if matches.get_flag(options::sym_links::FOLLOW_DIR_SYM_LINKS) {
            if matches.get_flag(options::dereference::NO_DEREFERENCE) {
                return Err(Error::ArgumentsMismatch(translate!(
                    "chcon-error-recursive-no-dereference-require-p"
                )));
            }

            (RecursiveMode::RecursiveAndFollowAllDirSymLinks, true)
        } else if matches.get_flag(options::sym_links::FOLLOW_ARG_DIR_SYM_LINK) {
            if matches.get_flag(options::dereference::NO_DEREFERENCE) {
                return Err(Error::ArgumentsMismatch(translate!(
                    "chcon-error-recursive-no-dereference-require-p"
                )));
            }

            (RecursiveMode::RecursiveAndFollowArgDirSymLinks, true)
        } else {
            if matches.get_flag(options::dereference::DEREFERENCE) {
                return Err(Error::ArgumentsMismatch(translate!(
                    "chcon-error-recursive-dereference-require-h-or-l"
                )));
            }

            (RecursiveMode::RecursiveButDoNotFollowSymLinks, false)
        }
    } else {
        let no_dereference = matches.get_flag(options::dereference::NO_DEREFERENCE);
        (RecursiveMode::NotRecursive, !no_dereference)
    };

    // By default, do not preserve root.
    let preserve_root = matches.get_flag(options::preserve_root::PRESERVE_ROOT);

    let mut files = matches.get_many::<OsString>("FILE").unwrap_or_default();

    let mode = if let Some(path) = matches.get_one::<OsString>(options::REFERENCE) {
        CommandLineMode::ReferenceBased {
            reference: PathBuf::from(path),
        }
    } else if matches.contains_id(options::USER)
        || matches.contains_id(options::ROLE)
        || matches.contains_id(options::TYPE)
        || matches.contains_id(options::RANGE)
    {
        CommandLineMode::Custom {
            user: matches.get_one::<OsString>(options::USER).map(Into::into),
            role: matches.get_one::<OsString>(options::ROLE).map(Into::into),
            the_type: matches.get_one::<OsString>(options::TYPE).map(Into::into),
            range: matches.get_one::<OsString>(options::RANGE).map(Into::into),
        }
    } else if let Some(context) = files.next() {
        CommandLineMode::ContextBased {
            context: context.into(),
        }
    } else {
        return Err(Error::MissingContext);
    };

    let files: Vec<_> = files.map(PathBuf::from).collect();
    if files.is_empty() {
        return Err(Error::MissingFiles);
    }

    Ok(Options {
        verbose,
        preserve_root,
        recursive_mode,
        affect_symlink_referent,
        mode,
        files,
    })
}

#[derive(Debug, Copy, Clone)]
enum RecursiveMode {
    NotRecursive,
    /// Do not traverse any symbolic links.
    RecursiveButDoNotFollowSymLinks,
    /// Traverse every symbolic link to a directory encountered.
    RecursiveAndFollowAllDirSymLinks,
    /// If a command line argument is a symbolic link to a directory, traverse it.
    RecursiveAndFollowArgDirSymLinks,
}

impl RecursiveMode {
    fn is_recursive(self) -> bool {
        match self {
            Self::NotRecursive => false,

            Self::RecursiveButDoNotFollowSymLinks
            | Self::RecursiveAndFollowAllDirSymLinks
            | Self::RecursiveAndFollowArgDirSymLinks => true,
        }
    }
}

#[derive(Debug)]
enum CommandLineMode {
    ReferenceBased {
        reference: PathBuf,
    },
    ContextBased {
        context: OsString,
    },
    Custom {
        user: Option<OsString>,
        role: Option<OsString>,
        the_type: Option<OsString>,
        range: Option<OsString>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DeviceAndINode {
    device_id: u64,
    inode: u64,
}

#[cfg(unix)]
impl From<fs::Metadata> for DeviceAndINode {
    fn from(md: fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;

        Self {
            device_id: md.dev(),
            inode: md.ino(),
        }
    }
}

impl From<&rustix::fs::Stat> for DeviceAndINode {
    #[allow(clippy::useless_conversion)]
    fn from(st: &rustix::fs::Stat) -> Self {
        Self {
            device_id: u64::from(st.st_dev),
            inode: u64::from(st.st_ino),
        }
    }
}

/// Open a handle on `name` (relative to `dir`) that identifies the object
/// without granting read or write access to it.
///
/// The handle pins the object: everything done afterwards goes through it,
/// so renaming or replacing the entry cannot redirect the relabel elsewhere.
fn open_object(dir: impl AsFd, name: &OsStr, follow: bool) -> io::Result<OwnedFd> {
    let mut flags = OFlags::PATH | OFlags::CLOEXEC;
    if !follow {
        flags |= OFlags::NOFOLLOW;
    }
    Ok(openat(dir, name, flags, Mode::empty())?)
}

/// A path naming the object behind `fd`, usable with the path-based libselinux calls.
///
/// Resolving it lands on the object the descriptor refers to, even when that
/// object is a symbolic link, so it is safe to use with the "follow" variants.
fn proc_fd_path(fd: BorrowedFd<'_>) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", fd.as_raw_fd()))
}

/// Pull the underlying OS error out of a libselinux failure, so that it can be
/// reported against the user's file name instead of the internal /proc path.
fn selinux_to_io(err: selinux::errors::Error) -> io::Error {
    use selinux::errors::Error as E;
    match err {
        E::IO { source, .. }
        | E::IO1Path { source, .. }
        | E::IO1Name { source, .. }
        | E::IO1Process { source, .. } => source,
        other => io::Error::other(other),
    }
}

/// One of the four fields of a security context, as selected by -u, -r, -t or -l.
#[derive(Debug, Clone, Copy)]
enum Field {
    User,
    Role,
    Type,
    Range,
}

/// What every file is relabeled to.
enum Target {
    /// A complete context, from CONTEXT or from --reference.
    Whole(CString),
    /// Fields to replace in each file's existing context.
    Fields(Vec<(Field, CString)>),
}

impl Target {
    fn new(options: &Options, context: &SELinuxSecurityContext<'_>) -> Result<Self> {
        if let CommandLineMode::Custom {
            user,
            role,
            the_type,
            range,
        } = &options.mode
        {
            let mut fields = Vec::new();
            for (field, value) in [
                (Field::User, user),
                (Field::Role, role),
                (Field::Type, the_type),
                (Field::Range, range),
            ] {
                if let Some(value) = value {
                    fields.push((field, os_str_to_c_string(value)?));
                }
            }
            return Ok(Self::Fields(fields));
        }

        match context.to_c_string()? {
            Some(text) => Ok(Self::Whole(text.into_owned())),
            None => Err(Error::from_io(
                translate!("chcon-op-creating-security-context"),
                io::Error::from_raw_os_error(libc::ENODATA),
            )),
        }
    }
}

/// Outcome of the run so far, and whether verbose output is still wanted.
struct Status {
    /// Cleared after the first failure to write the verbose output, so that
    /// the write error is only reported once.
    verbose: bool,
    failed: bool,
}

impl Status {
    fn report(&mut self, err: &Error) {
        show_error!("{}.", report_full_error(err));
        self.failed = true;
    }

    fn announce(&mut self, display: &Path) {
        if !self.verbose {
            return;
        }
        let line = translate!(
            "chcon-verbose-changing-context",
            "util_name" => uucore::util_name(),
            "file" => display.quote()
        );
        let mut out = io::stdout().lock();
        if let Err(e) = writeln!(out, "{line}").and_then(|()| out.flush()) {
            self.verbose = false;
            self.report(&Error::Write(e));
        }
    }
}

/// Walks the operands and relabels what they name.
struct Relabeler<'a> {
    options: &'a Options,
    target: Target,
    root_id: Option<DeviceAndINode>,
    status: Status,
    /// Identities of the directories currently being walked, outermost first.
    open_dirs: Vec<DeviceAndINode>,
}

impl Relabeler<'_> {
    fn run(&mut self) {
        let options = self.options;
        for path in &options.files {
            self.operand(path);
        }
    }

    fn operand(&mut self, path: &Path) {
        let recursive = self.options.recursive_mode.is_recursive();
        // In recursive mode, -H and -L both resolve a symbolic link given as an
        // operand, while -P labels the link itself. Otherwise -h decides.
        let follow = if recursive {
            !matches!(
                self.options.recursive_mode,
                RecursiveMode::RecursiveButDoNotFollowSymLinks
            )
        } else {
            self.options.affect_symlink_referent
        };

        let object = match open_object(rustix::fs::CWD, path.as_os_str(), follow) {
            Ok(fd) => fd,
            Err(e) => {
                let err = Error::from_io1(translate!("chcon-op-accessing"), path, e);
                return self.status.report(&err);
            }
        };

        if recursive {
            self.object(&object, path, false);
        } else {
            self.label(object.as_fd(), path);
        }
    }

    /// Handle an object reached during a recursive walk.
    ///
    /// `through_link` tells whether it was reached by resolving a symbolic link.
    fn object(&mut self, object: &OwnedFd, display: &Path, through_link: bool) {
        match rustix::fs::fstat(object) {
            Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::Directory => {
                self.directory(object, display, DeviceAndINode::from(&st), through_link);
            }
            Ok(_) => self.label(object.as_fd(), display),
            Err(e) => {
                let err =
                    Error::from_io1(translate!("chcon-op-getting-meta-data"), display, e.into());
                self.status.report(&err);
            }
        }
    }

    /// Relabel everything below a directory, then the directory itself.
    fn directory(
        &mut self,
        object: &OwnedFd,
        display: &Path,
        id: DeviceAndINode,
        through_link: bool,
    ) {
        if self.root_id == Some(id) {
            warn_recursive_root(display);
            self.status.failed = true;
            return;
        }

        if self.open_dirs.contains(&id) {
            // With -L, a link back to a directory that is being walked is
            // expected and is simply not descended. Reaching one without a
            // link means the file system itself loops.
            if !through_link {
                warn_directory_cycle(display);
                self.status.failed = true;
            }
            return self.label(object.as_fd(), display);
        }

        // Reopen the very same directory for reading, through its handle.
        let listing = openat(
            object,
            c".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(io::Error::from)
        .and_then(|fd| DirFd::from_raw_fd(fd.into_raw_fd()))
        .and_then(|dir| dir.read_dir().map(|names| (dir, names)));

        match listing {
            Ok((dir, names)) => {
                self.open_dirs.push(id);
                for name in names {
                    self.entry(&dir, &name, &display.join(&name));
                }
                self.open_dirs.pop();
            }
            Err(e) => {
                let err = Error::from_io1(translate!("chcon-op-reading-directory"), display, e);
                self.status.report(&err);
            }
        }

        self.label(object.as_fd(), display);
    }

    /// Handle one entry of a directory being walked.
    fn entry(&mut self, dir: &DirFd, name: &OsStr, display: &Path) {
        let accessing =
            |e: io::Error| Error::from_io1(translate!("chcon-op-accessing"), display, e);

        let entry = match open_object(dir, name, false) {
            Ok(fd) => fd,
            Err(e) => return self.status.report(&accessing(e)),
        };

        let is_link = match rustix::fs::fstat(&entry) {
            Ok(st) => FileType::from_raw_mode(st.st_mode) == FileType::Symlink,
            Err(e) => {
                let err =
                    Error::from_io1(translate!("chcon-op-getting-meta-data"), display, e.into());
                return self.status.report(&err);
            }
        };

        if !is_link {
            return self.object(&entry, display, false);
        }

        match self.options.recursive_mode {
            // -P: the link itself is relabeled.
            RecursiveMode::RecursiveButDoNotFollowSymLinks | RecursiveMode::NotRecursive => {
                self.label(entry.as_fd(), display);
            }
            // -H: links below an operand are not walked, but what they point
            // to is relabeled.
            RecursiveMode::RecursiveAndFollowArgDirSymLinks => match open_object(dir, name, true) {
                Ok(referent) => self.label(referent.as_fd(), display),
                Err(e) => self.status.report(&accessing(e)),
            },
            // -L: links are resolved, and directories they lead to are walked.
            RecursiveMode::RecursiveAndFollowAllDirSymLinks => match open_object(dir, name, true) {
                Ok(referent) => self.object(&referent, display, true),
                Err(e) => self.status.report(&accessing(e)),
            },
        }
    }

    /// Relabel the object behind `object`, named `display` in messages.
    fn label(&mut self, object: BorrowedFd<'_>, display: &Path) {
        let path = proc_fd_path(object);
        self.status.announce(display);

        let context: Cow<'_, CStr> = match &self.target {
            Target::Whole(text) => Cow::Borrowed(text),
            Target::Fields(fields) => match with_fields(&path, display, fields) {
                Ok(text) => Cow::Owned(text),
                Err(e) => return self.status.report(&e),
            },
        };

        if let Err(e) =
            SecurityContext::from_c_str(&context, false).set_for_path(&path, true, false)
        {
            let err = Error::from_io1(
                translate!("chcon-op-setting-security-context"),
                display,
                selinux_to_io(e),
            );
            self.status.report(&err);
        }
    }
}

/// The current context of the file at `path`, with `fields` replaced.
fn with_fields(path: &Path, display: &Path, fields: &[(Field, CString)]) -> Result<CString> {
    let building = |e| {
        Error::from_io1(
            translate!("chcon-op-creating-security-context"),
            display,
            selinux_to_io(e),
        )
    };

    let current = SecurityContext::of_path(path, true, false)
        .map_err(|e| {
            Error::from_io1(
                translate!("chcon-op-getting-security-context"),
                display,
                selinux_to_io(e),
            )
        })?
        // An unlabeled file has no fields to keep, so -u/-r/-t/-l cannot apply.
        .ok_or_else(|| {
            Error::from_io1(
                translate!("chcon-op-applying-partial-context"),
                display,
                io::Error::from_raw_os_error(libc::ENODATA),
            )
        })?;

    let current = current
        .to_c_string()
        .map_err(building)?
        .ok_or_else(|| building(selinux::errors::Error::UnexpectedSecurityContextFormat))?;
    let parts = OpaqueSecurityContext::from_c_str(&current).map_err(building)?;
    for (field, value) in fields {
        match field {
            Field::User => parts.set_user(value),
            Field::Role => parts.set_role(value),
            Field::Type => parts.set_type(value),
            Field::Range => parts.set_range(value),
        }
        .map_err(building)?;
    }
    parts.to_c_string().map_err(building)
}

/// Relabel every operand. Returns whether everything succeeded.
fn relabel_all(
    options: &Options,
    context: &SELinuxSecurityContext<'_>,
    root_id: Option<DeviceAndINode>,
) -> Result<bool> {
    let mut relabeler = Relabeler {
        options,
        target: Target::new(options, context)?,
        root_id,
        status: Status {
            verbose: options.verbose,
            failed: false,
        },
        open_dirs: Vec::new(),
    };
    relabeler.run();
    Ok(!relabeler.status.failed)
}

#[cfg(unix)]
pub(crate) fn os_str_to_c_string(s: &OsStr) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(s.as_bytes())
        .map_err(|_r| Error::from_io("CString::new()", io::ErrorKind::InvalidInput.into()))
}

/// Identify the root directory by device and inode, without following it.
#[cfg(unix)]
fn root_identity() -> Result<DeviceAndINode> {
    fs::symlink_metadata("/")
        .map(DeviceAndINode::from)
        .map_err(|r| Error::from_io1("std::fs::symlink_metadata", "/", r))
}

fn warn_recursive_root(dir_name: &Path) {
    if dir_name.as_os_str() == "/" {
        show_warning!(
            "{}",
            translate!("chcon-warning-dangerous-recursive-root", "option" => options::preserve_root::NO_PRESERVE_ROOT)
        );
    } else {
        show_warning!(
            "{}",
            translate!("chcon-warning-dangerous-recursive-dir", "dir" => dir_name.quote(), "option" => options::preserve_root::NO_PRESERVE_ROOT)
        );
    }
}

fn warn_directory_cycle(file_name: &Path) {
    show_warning!(
        "{}",
        translate!("chcon-warning-circular-directory", "file" => file_name.quote())
    );
}

#[derive(Debug)]
enum SELinuxSecurityContext<'t> {
    File(SecurityContext<'t>),
    String(Option<CString>),
}

impl SELinuxSecurityContext<'_> {
    fn to_c_string(&self) -> Result<Option<Cow<'_, CStr>>> {
        match self {
            Self::File(context) => context
                .to_c_string()
                .map_err(|r| Error::from_selinux("SELinuxSecurityContext::to_c_string()", r)),

            Self::String(context) => Ok(context.as_deref().map(Cow::Borrowed)),
        }
    }
}
