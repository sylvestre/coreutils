//  * This file is part of the uutils coreutils package.
//  *
//  * (c) Ben Eills <ben@beneills.com>
//  *
//  * For the full copyright and license information, please view the LICENSE file
//  * that was distributed with this source code.

// spell-checker:ignore (ToDO) rwxr sourcepath targetpath

extern crate getopts;
extern crate libc;

mod mode;

#[macro_use]
extern crate uucore;

use std::fs;
use std::path::{Path, PathBuf};
use std::result::Result;

static NAME: &str = "install";
static SUMMARY: &str = "Copy SOURCE to DEST or multiple SOURCE(s) to the existing
 DIRECTORY, while setting permission modes and owner/group";
static LONG_HELP: &str = "";

const DEFAULT_MODE: u32 = 755;

#[allow(dead_code)]
pub struct Behavior {
    main_function: MainFunction,
    specified_mode: Option<u32>,
    suffix: String,
    verbose: bool,
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
        match self.specified_mode {
            Some(x) => x,
            None => DEFAULT_MODE,
        }
    }
}

/// Main install utility function, called from main.rs.
///
/// Returns a program return code.
///
pub fn uumain(args: impl uucore::Args) -> i32 {
    let args = args.collect_str();

    let matches = parse_opts(args);

    if let Err(s) = check_unimplemented(&matches) {
        show_error!("Unimplemented feature: {}", s);
        return 2;
    }

    let behavior = match behavior(&matches) {
        Ok(x) => x,
        Err(ret) => {
            return ret;
        }
    };

    let paths: Vec<PathBuf> = {
        #[allow(clippy::ptr_arg)]
        fn string_to_path(s: &String) -> &Path {
            Path::new(s)
        };
        let to_owned = |p: &Path| p.to_owned();
        let arguments = matches.free.iter().map(string_to_path);

        arguments.map(to_owned).collect()
    };

    match behavior.main_function {
        MainFunction::Directory => directory(&paths[..], behavior),
        MainFunction::Standard => standard(&paths[..], behavior),
    }
}

/// Build a specification of the command line.
///
/// Returns a getopts::Options struct.
///
fn parse_opts(args: Vec<String>) -> getopts::Matches {
    let syntax = format!(
        "SOURCE DEST
 {} SOURCE... DIRECTORY",
        NAME
    );
    app!(&syntax, SUMMARY, LONG_HELP)
        // TODO implement flag
        .optflagopt(
            "",
            "backup",
            "(unimplemented) make a backup of each existing destination\n \
             file",
            "CONTROL",
        )
        // TODO implement flag
        .optflag(
            "b",
            "",
            "(unimplemented) like --backup but does not accept an argument",
        )
        .optflag("c", "", "ignored")
        // TODO implement flag
        .optflag(
            "C",
            "compare",
            "(unimplemented) compare each pair of source and destination\n \
             files, and in some cases, do not modify the destination at all",
        )
        .optflag(
            "d",
            "directory",
            "treat all arguments as directory names.\n \
             create all components of the specified directories",
        )
        // TODO implement flag
        .optflag(
            "D",
            "",
            "(unimplemented) create all leading components of DEST except the\n \
             last, then copy SOURCE to DEST",
        )
        // TODO implement flag
        .optflagopt(
            "g",
            "group",
            "(unimplemented) set group ownership, instead of process's\n \
             current group",
            "GROUP",
        )
        .optflagopt(
            "m",
            "mode",
            "set permission mode (as in chmod), instead\n \
             of rwxr-xr-x",
            "MODE",
        )
        // TODO implement flag
        .optflagopt(
            "o",
            "owner",
            "(unimplemented) set ownership (super-user only)",
            "OWNER",
        )
        // TODO implement flag
        .optflag(
            "p",
            "preserve-timestamps",
            "(unimplemented) apply access/modification times\n \
             of SOURCE files to corresponding destination files",
        )
        // TODO implement flag
        .optflag("s", "strip", "(unimplemented) strip symbol tables")
        // TODO implement flag
        .optflagopt(
            "",
            "strip-program",
            "(unimplemented) program used to strip binaries",
            "PROGRAM",
        )
        // TODO implement flag
        .optopt(
            "S",
            "suffix",
            "(unimplemented) override the usual backup suffix",
            "SUFFIX",
        )
        // TODO implement flag
        .optopt(
            "t",
            "target-directory",
            "(unimplemented) move all SOURCE arguments into\n \
             DIRECTORY",
            "DIRECTORY",
        )
        // TODO implement flag
        .optflag(
            "T",
            "no-target-directory",
            "(unimplemented) treat DEST as a normal file",
        )
        .optflag("v", "verbose", "explain what is being done")
        // TODO implement flag
        .optflag(
            "P",
            "preserve-context",
            "(unimplemented) preserve security context",
        )
        // TODO implement flag
        .optflagopt(
            "Z",
            "context",
            "(unimplemented) set security context of files and\n \
             directories",
            "CONTEXT",
        )
        .parse(args)
}

/// Check for unimplemented command line arguments.
///
/// Either return the degenerate Ok value, or an Err with string.
///
/// # Errors
///
/// Error datum is a string of the unimplemented argument.
///
fn check_unimplemented(matches: &getopts::Matches) -> Result<(), &str> {
    if matches.opt_present("backup") {
        Err("--backup")
    } else if matches.opt_present("b") {
        Err("-b")
    } else if matches.opt_present("compare") {
        Err("--compare, -C")
    } else if matches.opt_present("D") {
        Err("-D")
    } else if matches.opt_present("group") {
        Err("--group, -g")
    } else if matches.opt_present("owner") {
        Err("--owner, -o")
    } else if matches.opt_present("preserve-timestamps") {
        Err("--preserve-timestamps, -p")
    } else if matches.opt_present("strip") {
        Err("--strip, -s")
    } else if matches.opt_present("strip-program") {
        Err("--strip-program")
    } else if matches.opt_present("suffix") {
        Err("--suffix, -S")
    } else if matches.opt_present("target-directory") {
        Err("--target-directory, -t")
    } else if matches.opt_present("no-target-directory") {
        Err("--no-target-directory, -T")
    } else if matches.opt_present("preserve-context") {
        Err("--preserve-context, -P")
    } else if matches.opt_present("context") {
        Err("--context, -Z")
    } else {
        Ok(())
    }
}

/// Determine behavior, given command line arguments.
///
/// If successful, returns a filled-out Behavior struct.
///
/// # Errors
///
/// In event of failure, returns an integer intended as a program return code.
///
fn behavior(matches: &getopts::Matches) -> Result<Behavior, i32> {
    let main_function = if matches.opt_present("directory") {
        MainFunction::Directory
    } else {
        MainFunction::Standard
    };

    let considering_dir: bool = MainFunction::Directory == main_function;

    let specified_mode: Option<u32> = if matches.opt_present("mode") {
        match matches.opt_str("mode") {
            Some(x) => match mode::parse(&x[..], considering_dir) {
                Ok(y) => Some(y),
                Err(err) => {
                    show_error!("Invalid mode string: {}", err);
                    return Err(1);
                }
            },
            None => {
                show_error!(
                    "option '--mode' requires an argument\n \
                     Try '{} --help' for more information.",
                    NAME
                );
                return Err(1);
            }
        }
    } else {
        None
    };

    let backup_suffix = if matches.opt_present("suffix") {
        match matches.opt_str("suffix") {
            Some(x) => x,
            None => {
                show_error!(
                    "option '--suffix' requires an argument\n\
                     Try '{} --help' for more information.",
                    NAME
                );
                return Err(1);
            }
        }
    } else {
        "~".to_owned()
    };

    Ok(Behavior {
        main_function,
        specified_mode,
        suffix: backup_suffix,
        verbose: matches.opt_present("v"),
    })
}

/// Creates directories.
///
/// GNU man pages describe this functionality as creating 'all components of
/// the specified directories'.
///
/// Returns an integer intended as a program return code.
///
fn directory(paths: &[PathBuf], b: Behavior) -> i32 {
    if paths.is_empty() {
        println!("{} with -d requires at least one argument.", NAME);
        1
    } else {
        let mut all_successful = true;

        for directory in paths.iter() {
            let path = directory.as_path();

            if path.exists() {
                show_info!("cannot create directory '{}': File exists", path.display());
                all_successful = false;
            }

            if let Err(e) = fs::create_dir(directory) {
                show_info!("{}: {}", path.display(), e.to_string());
                all_successful = false;
            }

            if mode::chmod(&path, b.mode()).is_err() {
                all_successful = false;
            }

            if b.verbose {
                show_info!("created directory '{}'", path.display());
            }
        }
        if all_successful {
            0
        } else {
            1
        }
    }
}

/// Test if the path is a a new file path that can be
/// created immediately
fn is_new_file_path(path: &Path) -> bool {
    path.is_file() || !path.exists() && path.parent().map(Path::is_dir).unwrap_or(true)
}

/// Perform an install, given a list of paths and behavior.
///
/// Returns an integer intended as a program return code.
///
fn standard(paths: &[PathBuf], b: Behavior) -> i32 {
    if paths.len() < 2 {
        println!("{} requires at least 2 arguments.", NAME);
        1
    } else {
        let sources = &paths[0..paths.len() - 1];
        let target = &paths[paths.len() - 1];

        if (target.is_file() || is_new_file_path(target)) && sources.len() == 1 {
            copy_file_to_file(&sources[0], target, &b)
        } else {
            copy_files_into_dir(sources, target, &b)
        }
    }
}

/// Copy some files into a directory.
///
/// Prints verbose information and error messages.
/// Returns an integer intended as a program return code.
///
/// # Parameters
///
/// _files_ must all exist as non-directories.
/// _target_dir_ must be a directory.
///
fn copy_files_into_dir(files: &[PathBuf], target_dir: &PathBuf, b: &Behavior) -> i32 {
    if !target_dir.is_dir() {
        show_error!("target ‘{}’ is not a directory", target_dir.display());
        return 1;
    }

    let mut all_successful = true;
    for sourcepath in files.iter() {
        let targetpath = match sourcepath.as_os_str().to_str() {
            Some(name) => target_dir.join(name),
            None => {
                show_error!(
                    "cannot stat ‘{}’: No such file or directory",
                    sourcepath.display()
                );

                all_successful = false;
                continue;
            }
        };

        if copy(sourcepath, &targetpath, b).is_err() {
            all_successful = false;
        }
    }
    if all_successful {
        0
    } else {
        1
    }
}

/// Copy a file to another file.
///
/// Prints verbose information and error messages.
/// Returns an integer intended as a program return code.
///
/// # Parameters
///
/// _file_ must exist as a non-directory.
/// _target_ must be a non-directory
///
fn copy_file_to_file(file: &PathBuf, target: &PathBuf, b: &Behavior) -> i32 {
    if copy(file, &target, b).is_err() {
        1
    } else {
        0
    }
}

/// Copy one file to a new location, changing metadata.
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
fn copy(from: &PathBuf, to: &PathBuf, b: &Behavior) -> Result<(), ()> {
    let io_result = fs::copy(from, to);

    if let Err(err) = io_result {
        show_error!(
            "install: cannot install ‘{}’ to ‘{}’: {}",
            from.display(),
            to.display(),
            err
        );
        return Err(());
    }

    if mode::chmod(&to, b.mode()).is_err() {
        return Err(());
    }

    if b.verbose {
        show_info!("'{}' -> '{}'", from.display(), to.display());
    }

    Ok(())
}
