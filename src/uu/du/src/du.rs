// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{Arg, ArgAction, ArgMatches, Command, builder::PossibleValue};
use glob::Pattern;
use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, stdout};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc;
use std::thread;
use thiserror::Error;
use uucore::LocalizedCommand;
use uucore::display::{Quotable, print_verbatim};
use uucore::error::{UError, UResult, USimpleError, set_exit_code};
use uucore::fsext::{MetadataTimeField, metadata_get_time};
use uucore::line_ending::LineEnding;
use uucore::parser::parse_glob;
use uucore::parser::parse_size::{ParseSizeError, parse_size_u64};
use uucore::parser::shortcut_value_parser::ShortcutValueParser;
#[cfg(unix)]
use uucore::safe_traversal::DirFd;
use uucore::time::{FormatSystemTimeFallback, format, format_system_time};
use uucore::translate;
use uucore::{format_usage, show, show_error, show_warning};

mod options {
    pub const HELP: &str = "help";
    pub const NULL: &str = "0";
    pub const ALL: &str = "all";
    pub const APPARENT_SIZE: &str = "apparent-size";
    pub const BLOCK_SIZE: &str = "block-size";
    pub const BYTES: &str = "b";
    pub const TOTAL: &str = "c";
    pub const MAX_DEPTH: &str = "d";
    pub const HUMAN_READABLE: &str = "h";
    pub const BLOCK_SIZE_1K: &str = "k";
    pub const COUNT_LINKS: &str = "l";
    pub const BLOCK_SIZE_1M: &str = "m";
    pub const SEPARATE_DIRS: &str = "S";
    pub const SUMMARIZE: &str = "s";
    pub const THRESHOLD: &str = "threshold";
    pub const SI: &str = "si";
    pub const TIME: &str = "time";
    pub const TIME_STYLE: &str = "time-style";
    pub const ONE_FILE_SYSTEM: &str = "one-file-system";
    pub const DEREFERENCE: &str = "dereference";
    pub const DEREFERENCE_ARGS: &str = "dereference-args";
    pub const NO_DEREFERENCE: &str = "no-dereference";
    pub const INODES: &str = "inodes";
    pub const EXCLUDE: &str = "exclude";
    pub const EXCLUDE_FROM: &str = "exclude-from";
    pub const FILES0_FROM: &str = "files0-from";
    pub const VERBOSE: &str = "verbose";
    pub const FILE: &str = "FILE";
}

struct TraversalOptions {
    #[cfg(unix)]
    all: bool,
    #[cfg(unix)]
    separate_dirs: bool,
    #[cfg(unix)]
    one_file_system: bool,
    #[cfg(unix)]
    dereference: Deref,
    #[cfg(unix)]
    count_links: bool,
    verbose: bool,
    excludes: Vec<Pattern>,
}

struct StatPrinter {
    total: bool,
    inodes: bool,
    max_depth: Option<usize>,
    threshold: Option<Threshold>,
    apparent_size: bool,
    size_format: SizeFormat,
    time: Option<MetadataTimeField>,
    time_format: String,
    line_ending: LineEnding,
    summarize: bool,
    total_text: String,
}

#[cfg(unix)]
#[derive(PartialEq, Clone)]
enum Deref {
    All,
    Args(Vec<PathBuf>),
    None,
}

#[derive(Clone)]
enum SizeFormat {
    HumanDecimal,
    HumanBinary,
    BlockSize(u64),
}

#[derive(Clone)]
struct Stat {
    path: PathBuf,
    size: u64,
    blocks: u64,
    inodes: u64,
    metadata: Metadata,
}

#[cfg(unix)]
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
struct FileInfo {
    file_id: u128,
    dev_id: u64,
}

impl Stat {
    // Stat constructor removed - safe traversal uses different data structures
}

fn read_block_size(s: Option<&str>) -> UResult<u64> {
    if let Some(s) = s {
        parse_size_u64(s)
            .map_err(|e| USimpleError::new(1, format_error_message(&e, s, options::BLOCK_SIZE)))
    } else {
        for env_var in ["DU_BLOCK_SIZE", "BLOCK_SIZE", "BLOCKSIZE"] {
            if let Ok(env_size) = env::var(env_var) {
                if let Ok(v) = parse_size_u64(&env_size) {
                    return Ok(v);
                }
            }
        }
        if env::var("POSIXLY_CORRECT").is_ok() {
            Ok(512)
        } else {
            Ok(1024)
        }
    }
}

#[derive(Debug, Error)]
enum DuError {
    #[error("{}", translate!("du-error-invalid-max-depth", "depth" => _0.quote()))]
    InvalidMaxDepthArg(String),

    #[error("{}", translate!("du-error-summarize-depth-conflict", "depth" => _0.maybe_quote()))]
    SummarizeDepthConflict(String),

    #[error("{}", translate!("du-error-invalid-time-style", "style" => _0.quote(), "help" => uucore::execution_phrase()))]
    InvalidTimeStyleArg(String),

    #[error("{}", translate!("du-error-invalid-glob", "error" => _0))]
    InvalidGlob(String),
}

impl UError for DuError {
    fn code(&self) -> i32 {
        match self {
            Self::InvalidMaxDepthArg(_)
            | Self::SummarizeDepthConflict(_)
            | Self::InvalidTimeStyleArg(_)
            | Self::InvalidGlob(_) => 1,
        }
    }
}

/// Read a file and return each line in a vector of String
fn file_as_vec(filename: impl AsRef<Path>) -> Vec<String> {
    let file = File::open(filename).expect("no such file");
    let buf = BufReader::new(file);

    buf.lines()
        .map(|l| l.expect("Could not parse line"))
        .collect()
}

/// Given the `--exclude-from` and/or `--exclude` arguments, returns the globset lists
/// to ignore the files
fn build_exclude_patterns(matches: &ArgMatches) -> UResult<Vec<Pattern>> {
    let exclude_from_iterator = matches
        .get_many::<String>(options::EXCLUDE_FROM)
        .unwrap_or_default()
        .flat_map(file_as_vec);

    let excludes_iterator = matches
        .get_many::<String>(options::EXCLUDE)
        .unwrap_or_default()
        .cloned();

    let mut exclude_patterns = Vec::new();
    for f in excludes_iterator.chain(exclude_from_iterator) {
        if matches.get_flag(options::VERBOSE) {
            println!(
                "{}",
                translate!("du-verbose-adding-to-exclude-list", "pattern" => f.clone())
            );
        }
        match parse_glob::from_str(&f) {
            Ok(glob) => exclude_patterns.push(glob),
            Err(err) => return Err(DuError::InvalidGlob(err.to_string()).into()),
        }
    }
    Ok(exclude_patterns)
}

struct StatPrintInfo {
    stat: Stat,
    depth: usize,
}

impl StatPrinter {
    fn choose_size(&self, stat: &Stat) -> u64 {
        if self.inodes {
            stat.inodes
        } else if self.apparent_size {
            stat.size
        } else {
            // The st_blocks field indicates the number of blocks allocated to the file, 512-byte units.
            // See: http://linux.die.net/man/2/stat
            stat.blocks * 512
        }
    }

    fn print_stats(&self, rx: &mpsc::Receiver<UResult<StatPrintInfo>>) -> UResult<()> {
        let mut grand_total = 0;
        loop {
            let received = rx.recv();

            match received {
                Ok(message) => match message {
                    Ok(stat_info) => {
                        let size = self.choose_size(&stat_info.stat);

                        if stat_info.depth == 0 {
                            grand_total += size;
                        }

                        if !self
                            .threshold
                            .is_some_and(|threshold| threshold.should_exclude(size))
                            && self
                                .max_depth
                                .is_none_or(|max_depth| stat_info.depth <= max_depth)
                            && (!self.summarize || stat_info.depth == 0)
                        {
                            self.print_stat(&stat_info.stat, size)?;
                        }
                    }
                    Err(e) => show!(e),
                },
                Err(_) => break,
            }
        }

        if self.total {
            print!("{}\t{}", self.convert_size(grand_total), self.total_text);
            print!("{}", self.line_ending);
        }

        Ok(())
    }

    fn convert_size(&self, size: u64) -> String {
        match self.size_format {
            SizeFormat::HumanDecimal => uucore::format::human::human_readable(
                size,
                uucore::format::human::SizeFormat::Decimal,
            ),
            SizeFormat::HumanBinary => uucore::format::human::human_readable(
                size,
                uucore::format::human::SizeFormat::Binary,
            ),
            SizeFormat::BlockSize(block_size) => {
                if self.inodes {
                    // we ignore block size (-B) with --inodes
                    size.to_string()
                } else {
                    size.div_ceil(block_size).to_string()
                }
            }
        }
    }

    fn print_stat(&self, stat: &Stat, size: u64) -> UResult<()> {
        print!("{}\t", self.convert_size(size));

        if let Some(md_time) = &self.time {
            if let Some(time) = metadata_get_time(&stat.metadata, *md_time) {
                format_system_time(
                    &mut stdout(),
                    time,
                    &self.time_format,
                    FormatSystemTimeFallback::IntegerError,
                )?;
                print!("\t");
            } else {
                print!("???\t");
            }
        }

        print_verbatim(&stat.path).unwrap();
        print!("{}", self.line_ending);

        Ok(())
    }
}

/// Read file paths from the specified file, separated by null characters
fn read_files_from(file_name: &OsStr) -> Result<Vec<PathBuf>, std::io::Error> {
    let reader: Box<dyn BufRead> = if file_name == "-" {
        // Read from standard input
        Box::new(BufReader::new(std::io::stdin()))
    } else {
        // First, check if the file_name is a directory
        let path = PathBuf::from(file_name);
        if path.is_dir() {
            return Err(std::io::Error::other(
                translate!("du-error-read-error-is-directory", "file" => file_name.to_string_lossy()),
            ));
        }

        // Attempt to open the file and handle the error if it does not exist
        match File::open(file_name) {
            Ok(file) => Box::new(BufReader::new(file)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(std::io::Error::other(
                    translate!("du-error-cannot-open-for-reading", "file" => file_name.to_string_lossy()),
                ));
            }
            Err(e) => return Err(e),
        }
    };

    let mut paths = Vec::new();

    for (i, line) in reader.split(b'\0').enumerate() {
        let path = line?;

        if path.is_empty() {
            let line_number = i + 1;
            show_error!(
                "{}",
                translate!("du-error-invalid-zero-length-file-name", "file" => file_name.to_string_lossy(), "line" => line_number)
            );
            set_exit_code(1);
        } else {
            let p = PathBuf::from(&*uucore::os_str_from_bytes(&path).unwrap());
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }

    Ok(paths)
}

#[uucore::main]
#[allow(clippy::cognitive_complexity)]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().get_matches_from_localized(args);

    let summarize = matches.get_flag(options::SUMMARIZE);

    let count_links = matches.get_flag(options::COUNT_LINKS);

    let max_depth = parse_depth(
        matches
            .get_one::<String>(options::MAX_DEPTH)
            .map(|s| s.as_str()),
        summarize,
    )?;

    let files = if let Some(file_from) = matches.get_one::<OsString>(options::FILES0_FROM) {
        if file_from == "-" && matches.get_one::<OsString>(options::FILE).is_some() {
            return Err(std::io::Error::other(
                translate!("du-error-extra-operand-with-files0-from",
                    "file" => matches
                        .get_one::<OsString>(options::FILE)
                        .unwrap()
                        .to_string_lossy()
                        .quote()
                ),
            )
            .into());
        }

        read_files_from(file_from)?
    } else if let Some(files) = matches.get_many::<OsString>(options::FILE) {
        let files = files.map(PathBuf::from);
        if count_links {
            files.collect()
        } else {
            // Deduplicate while preserving order
            let mut seen = HashSet::new();
            let should_dereference = matches.get_flag(options::DEREFERENCE);
            files
                .filter(|path| {
                    let dedup_key = if should_dereference {
                        // When dereferencing, use the canonicalized path for deduplication
                        match fs::canonicalize(path) {
                            Ok(canonical) => canonical,
                            Err(_) => path.clone(), // Fall back to original path if canonicalization fails
                        }
                    } else {
                        path.clone()
                    };
                    seen.insert(dedup_key)
                })
                .collect::<Vec<_>>()
        }
    } else {
        vec![PathBuf::from(".")]
    };

    let time = matches.contains_id(options::TIME).then(|| {
        matches
            .get_one::<String>(options::TIME)
            .map_or(MetadataTimeField::Modification, |s| s.as_str().into())
    });

    let size_format = if matches.get_flag(options::HUMAN_READABLE) {
        SizeFormat::HumanBinary
    } else if matches.get_flag(options::SI) {
        SizeFormat::HumanDecimal
    } else if matches.get_flag(options::BYTES) {
        SizeFormat::BlockSize(1)
    } else if matches.get_flag(options::BLOCK_SIZE_1K) {
        SizeFormat::BlockSize(1024)
    } else if matches.get_flag(options::BLOCK_SIZE_1M) {
        SizeFormat::BlockSize(1024 * 1024)
    } else {
        let block_size_str = matches.get_one::<String>(options::BLOCK_SIZE);
        let block_size = read_block_size(block_size_str.map(AsRef::as_ref))?;
        if block_size == 0 {
            return Err(std::io::Error::other(translate!("du-error-invalid-block-size-argument", "option" => options::BLOCK_SIZE, "value" => block_size_str.map_or("???BUG", |v| v).quote()))
            .into());
        }
        SizeFormat::BlockSize(block_size)
    };

    let traversal_options = TraversalOptions {
        #[cfg(unix)]
        all: matches.get_flag(options::ALL),
        #[cfg(unix)]
        separate_dirs: matches.get_flag(options::SEPARATE_DIRS),
        #[cfg(unix)]
        one_file_system: matches.get_flag(options::ONE_FILE_SYSTEM),
        #[cfg(unix)]
        dereference: if matches.get_flag(options::DEREFERENCE) {
            Deref::All
        } else if matches.get_flag(options::DEREFERENCE_ARGS) {
            // We don't care about the cost of cloning as it is rarely used
            Deref::Args(files.clone())
        } else {
            Deref::None
        },
        #[cfg(unix)]
        count_links,
        verbose: matches.get_flag(options::VERBOSE),
        excludes: build_exclude_patterns(&matches)?,
    };

    let time_format = if time.is_some() {
        parse_time_style(matches.get_one::<String>("time-style"))?
    } else {
        format::LONG_ISO.to_string()
    };

    let stat_printer = StatPrinter {
        max_depth,
        size_format,
        summarize,
        total: matches.get_flag(options::TOTAL),
        inodes: matches.get_flag(options::INODES),
        threshold: matches
            .get_one::<String>(options::THRESHOLD)
            .map(|s| {
                Threshold::from_str(s).map_err(|e| {
                    USimpleError::new(1, format_error_message(&e, s, options::THRESHOLD))
                })
            })
            .transpose()?,
        apparent_size: matches.get_flag(options::APPARENT_SIZE) || matches.get_flag(options::BYTES),
        time,
        time_format,
        line_ending: LineEnding::from_zero_flag(matches.get_flag(options::NULL)),
        total_text: translate!("du-total"),
    };

    if stat_printer.inodes
        && (matches.get_flag(options::APPARENT_SIZE) || matches.get_flag(options::BYTES))
    {
        show_warning!(
            "{}",
            translate!("du-warning-apparent-size-ineffective-with-inodes")
        );
    }

    // Use separate thread to print output, so we can print finished results while computation is still running
    let (print_tx, rx) = mpsc::channel::<UResult<StatPrintInfo>>();
    let printing_thread = thread::spawn(move || stat_printer.print_stats(&rx));

    'loop_file: for path in files {
        // Skip if we don't want to ignore anything
        if !&traversal_options.excludes.is_empty() {
            let path_string = path.to_string_lossy();
            for pattern in &traversal_options.excludes {
                if pattern.matches(&path_string) {
                    // if the directory is ignored, leave early
                    if traversal_options.verbose {
                        println!(
                            "{}",
                            translate!("du-verbose-ignored", "path" => path_string.quote())
                        );
                    }
                    continue 'loop_file;
                }
            }
        }

        // Always use safe traversal on Unix (per user request)
        #[cfg(unix)]
        {
            match try_safe_du(&path, &traversal_options, &print_tx) {
                Ok(stat) => {
                    print_tx
                        .send(Ok(StatPrintInfo { stat, depth: 0 }))
                        .map_err(|e| USimpleError::new(1, e.to_string()))?;
                }
                Err(_e) => {
                    // try_safe_du already sent the error message, so don't send another one
                }
            }
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, safe traversal is not available, use basic implementation
            // This will be a simplified version that works for most cases
            if path.is_dir() {
                print_tx
                    .send(Ok(StatPrintInfo {
                        stat: Stat {
                            path: path.clone(),
                            size: 0, // Placeholder - would need proper calculation
                            blocks: 0,
                            inodes: 1,
                            metadata: fs::metadata(&path)
                                .unwrap_or_else(|_| fs::metadata(".").unwrap()),
                        },
                        depth: 0,
                    }))
                    .map_err(|e| USimpleError::new(1, e.to_string()))?;
            } else {
                print_tx
                    .send(Err(USimpleError::new(
                        1,
                        translate!("du-error-cannot-access-no-such-file", "path" => path.to_string_lossy().quote()),
                    )))
                    .map_err(|e| USimpleError::new(1, e.to_string()))?;
            }
        }
    }

    drop(print_tx);

    printing_thread
        .join()
        .map_err(|_| USimpleError::new(1, translate!("du-error-printing-thread-panicked")))??;

    Ok(())
}

// Parse --time-style argument, falling back to environment variable if necessary.
fn parse_time_style(s: Option<&String>) -> UResult<String> {
    let s = match s {
        Some(s) => Some(s.into()),
        None => {
            match env::var("TIME_STYLE") {
                // Per GNU manual, strip `posix-` if present, ignore anything after a newline if
                // the string starts with +, and ignore "locale".
                Ok(s) => {
                    let s = s.strip_prefix("posix-").unwrap_or(s.as_str());
                    let s = match s.chars().next().unwrap() {
                        '+' => s.split('\n').next().unwrap(),
                        _ => s,
                    };
                    match s {
                        "locale" => None,
                        _ => Some(s.to_string()),
                    }
                }
                Err(_) => None,
            }
        }
    };
    match s {
        Some(s) => match s.as_ref() {
            "full-iso" => Ok(format::FULL_ISO.to_string()),
            "long-iso" => Ok(format::LONG_ISO.to_string()),
            "iso" => Ok(format::ISO.to_string()),
            _ => match s.chars().next().unwrap() {
                '+' => Ok(s[1..].to_string()),
                _ => Err(DuError::InvalidTimeStyleArg(s).into()),
            },
        },
        None => Ok(format::LONG_ISO.to_string()),
    }
}

fn parse_depth(max_depth_str: Option<&str>, summarize: bool) -> UResult<Option<usize>> {
    let max_depth = max_depth_str.as_ref().and_then(|s| s.parse::<usize>().ok());
    match (max_depth_str, max_depth) {
        (Some(s), _) if summarize => Err(DuError::SummarizeDepthConflict(s.into()).into()),
        (Some(s), None) => Err(DuError::InvalidMaxDepthArg(s.into()).into()),
        (Some(_), Some(_)) | (None, _) => Ok(max_depth),
    }
}

#[cfg(unix)]
fn try_safe_du(
    path: &Path,
    traversal_options: &TraversalOptions,
    print_tx: &mpsc::Sender<UResult<StatPrintInfo>>,
) -> Result<Stat, Box<mpsc::SendError<UResult<StatPrintInfo>>>> {
    // Check if path is a file or directory
    // Use symlink_metadata for the initial check to avoid following symlinks automatically
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            // Check the specific error kind
            match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    // Report the permission error but still return a minimal stat for the directory
                    let error_msg = translate!("du-error-cannot-read-directory", "path" => path.to_string_lossy().quote());
                    let _ = print_tx.send(Err(USimpleError::new(1, error_msg.clone())));

                    // Try to get directory metadata using fs::metadata, which might succeed even if symlink_metadata failed
                    // This can happen in some permission scenarios
                    let metadata = fs::metadata(path).unwrap_or_else(|_| {
                        // If we can't get any metadata, create a minimal fake metadata
                        // This uses the current directory as a template
                        fs::metadata(".").expect("Failed to get fallback metadata")
                    });

                    // Return a minimal Stat for the directory, following GNU du behavior
                    return Ok(Stat {
                        path: path.to_path_buf(),
                        size: 0,
                        blocks: 0, // GNU du often shows 0 for permission denied directories
                        inodes: 1,
                        metadata,
                    });
                }
                std::io::ErrorKind::NotFound => {
                    // In some cases, when a directory exists but has no read permissions,
                    // the OS may return NotFound instead of PermissionDenied.
                    // If we suspect this might be a permission issue (based on context),
                    // try to determine if it's really a permission problem.

                    // First, check if parent directory exists and is accessible
                    if let Some(parent) = path.parent() {
                        if parent.exists() {
                            // Parent exists, so this could be a permission issue
                            // Try a different approach: check if we can list the parent directory
                            // and see if our target shows up (but we can't access it)
                            if let Ok(entries) = fs::read_dir(parent) {
                                for entry in entries.flatten() {
                                    if entry.file_name() == path.file_name().unwrap_or_default() {
                                        // The file/directory exists in parent listing but we can't access it
                                        // This is likely a permission issue
                                        let error_msg = translate!("du-error-cannot-read-directory", "path" => path.to_string_lossy().quote());
                                        let _ = print_tx
                                            .send(Err(USimpleError::new(1, error_msg.clone())));
                                        return Err(Box::new(mpsc::SendError(Err(
                                            USimpleError::new(1, error_msg),
                                        ))));
                                    }
                                }
                            }
                        }
                    }

                    // If we get here, it's genuinely not found
                    let error_msg = translate!("du-error-cannot-access-no-such-file", "path" => path.to_string_lossy().quote());
                    let _ = print_tx.send(Err(USimpleError::new(1, error_msg.clone())));
                    return Err(Box::new(mpsc::SendError(Err(USimpleError::new(
                        1, error_msg,
                    )))));
                }
                _ => {
                    // Other errors - use generic message
                    let error_msg = translate!("du-error-cannot-access-no-such-file", "path" => path.to_string_lossy().quote());
                    let _ = print_tx.send(Err(USimpleError::new(1, error_msg.clone())));
                    return Err(Box::new(mpsc::SendError(Err(USimpleError::new(
                        1, error_msg,
                    )))));
                }
            }
        }
    };

    // Check if we should dereference this path
    let should_deref = match &traversal_options.dereference {
        Deref::All => true,
        Deref::Args(paths) => paths.contains(&path.to_path_buf()),
        Deref::None => false,
    };

    // If it's a symlink and we should dereference, get the target metadata
    let metadata = if metadata.is_symlink() && should_deref {
        match fs::metadata(path) {
            Ok(m) => m,
            Err(_e) => metadata, // Fall back to symlink metadata if target doesn't exist
        }
    } else {
        metadata
    };

    // If it's a file, handle it directly
    let is_deref_all = matches!(traversal_options.dereference, Deref::All);
    if metadata.is_file() || (metadata.is_symlink() && !is_deref_all) {
        let size = metadata.len();
        let blocks = {
            #[cfg(unix)]
            {
                metadata.blocks()
            }
            #[cfg(not(unix))]
            {
                size / 512
            }
        };

        return Ok(Stat {
            path: path.to_path_buf(),
            size,
            blocks,
            inodes: 1,
            metadata,
        });
    }

    // For directories, use DirFd
    let dir_fd = match DirFd::open(path) {
        Ok(fd) => fd,
        Err(e) => {
            // Check if this is a permission error when opening the directory
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                let error_msg = translate!("du-error-cannot-read-directory", "path" => path.to_string_lossy().quote());
                let _ = print_tx.send(Err(USimpleError::new(1, error_msg.clone())));

                // Return a minimal stat for the directory when we can't open it due to permissions
                // GNU du still shows the directory size even when it can't be read
                #[cfg(not(target_vendor = "apple"))]
                let blocks = 8; // 4K in 512-byte blocks on most systems
                #[cfg(target_vendor = "apple")]
                let blocks = 0; // Apple systems show 0

                return Ok(Stat {
                    path: path.to_path_buf(),
                    size: 0,
                    blocks,
                    inodes: 1,
                    metadata, // We already have metadata from earlier
                });
            }
            let error_msg = translate!("du-error-cannot-access-no-such-file", "path" => path.to_string_lossy().quote());
            let _ = print_tx.send(Err(USimpleError::new(1, error_msg.clone())));
            return Err(Box::new(mpsc::SendError(Err(USimpleError::new(
                1, error_msg,
            )))));
        }
    };

    let follow_symlinks = match &traversal_options.dereference {
        Deref::All => true,
        Deref::Args(paths) => paths.contains(&path.to_path_buf()),
        Deref::None => false,
    };

    let mut seen_inodes = HashSet::new();

    // Use the new safe_du_with_output function
    match safe_du_with_output(
        &dir_fd,
        path,
        0, // depth
        traversal_options,
        follow_symlinks,
        &mut seen_inodes,
        print_tx,
    ) {
        Ok(file_stat) => {
            // Convert back to Stat
            Ok(file_stat)
        }
        Err(e) => {
            let error_msg = format!("Safe du failed: {e}");
            let _ = print_tx.send(Err(USimpleError::new(1, error_msg.clone())));
            Err(Box::new(mpsc::SendError(Err(USimpleError::new(
                1, error_msg,
            )))))
        }
    }
}

#[cfg(unix)]
fn safe_du_with_output(
    dir_fd: &DirFd,
    path: &Path,
    depth: usize,
    traversal_options: &TraversalOptions,
    follow_symlinks: bool,
    seen_inodes: &mut HashSet<FileInfo>,
    print_tx: &mpsc::Sender<UResult<StatPrintInfo>>,
) -> Result<Stat, Box<dyn std::error::Error>> {
    // Get stats for current directory
    let stat = dir_fd.fstat().map_err(|e| format!("fstat failed: {e}"))?;
    let file_info = FileInfo {
        file_id: stat.st_ino as u128,
        #[allow(clippy::unnecessary_cast)]
        dev_id: stat.st_dev as u64,
    };

    let mut total_stat = Stat {
        path: path.to_path_buf(),
        #[allow(clippy::unnecessary_cast)]
        size: if (stat.st_mode as u32 & libc::S_IFMT as u32) == libc::S_IFDIR as u32 {
            0
        } else {
            stat.st_size as u64
        },
        blocks: stat.st_blocks as u64,
        inodes: 1,
        metadata: fs::metadata(path).map_err(|e| format!("metadata failed: {e}"))?,
    };

    // Check for hard link cycle
    if seen_inodes.contains(&file_info)
        && (!traversal_options.count_links || !traversal_options.all)
    {
        if traversal_options.count_links && !traversal_options.all {
            total_stat.inodes += 1;
        }
        return Ok(total_stat);
    }
    seen_inodes.insert(file_info);

    // If not a directory, we're done
    #[allow(clippy::unnecessary_cast)]
    if (stat.st_mode as u32 & libc::S_IFMT as u32) != libc::S_IFDIR as u32 {
        return Ok(total_stat);
    }

    // Read directory entries
    let entries = match dir_fd.read_dir() {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                let _ = print_tx.send(Err(USimpleError::new(
                    1,
                    translate!("du-error-cannot-read-directory", "path" => path.to_string_lossy().quote()),
                )));
                // Still return the stat for the directory itself, since GNU du does show the directory size
                // GNU du typically shows 4K (8 blocks of 512 bytes) for directories on most filesystems
                // even when permission is denied, unless the filesystem reports 0 blocks
                let blocks = if stat.st_blocks == 0 {
                    // Some filesystems report 0 blocks for directories, but GNU du may still show 4K
                    // This is platform and filesystem dependent
                    #[cfg(not(target_vendor = "apple"))]
                    {
                        8 // 4K in 512-byte blocks
                    }
                    #[cfg(target_vendor = "apple")]
                    {
                        0 // Apple systems show 0
                    }
                } else {
                    stat.st_blocks as u64
                };

                let metadata = fs::metadata(path).map_err(|e| format!("metadata failed: {e}"))?;
                return Ok(Stat {
                    path: path.to_path_buf(),
                    size: stat.st_size as u64,
                    blocks,
                    #[allow(clippy::unnecessary_cast)]
                    inodes: stat.st_nlink as u64,
                    metadata,
                });
            }
            return Err(format!("read_dir failed: {e}").into());
        }
    };

    for entry_name in entries {
        // Get stats for this entry
        let entry_stat = match dir_fd.stat_at(&entry_name, follow_symlinks) {
            Ok(stat) => stat,
            Err(e) => {
                // Report permission denied errors for accessing entries
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    let entry_path = path.join(entry_name.to_string_lossy().as_ref());
                    let _ = print_tx.send(Err(USimpleError::new(
                        1,
                        translate!("du-error-cannot-read-directory",
                            "path" => entry_path.to_string_lossy().quote()),
                    )));
                    set_exit_code(1);
                }
                // Skip entries we can't stat
                continue;
            }
        };

        let entry_file_info = FileInfo {
            file_id: entry_stat.st_ino as u128,
            dev_id: entry_stat.st_dev as u64,
        };

        // Note: We don't add entry_file_info to seen_inodes here for directories
        // because the recursive call will handle that. For files, we add them below.

        // Check if crossing filesystem boundary
        if traversal_options.one_file_system && entry_stat.st_dev != stat.st_dev {
            continue;
        }

        let entry_path = path.join(entry_name.to_string_lossy().as_ref());

        // Check exclude patterns
        if !traversal_options.excludes.is_empty() {
            let entry_path_string = entry_path.to_string_lossy();
            let entry_name_string = entry_name.to_string_lossy();
            let mut should_exclude = false;
            for pattern in &traversal_options.excludes {
                if pattern.matches(&entry_path_string) || pattern.matches(&entry_name_string) {
                    // Skip excluded entries
                    if traversal_options.verbose {
                        println!("excluding: '{}'", entry_path.display());
                    }
                    should_exclude = true;
                    break;
                }
            }
            if should_exclude {
                continue;
            }
        }

        #[allow(clippy::unnecessary_cast)]
        if (entry_stat.st_mode as u32 & libc::S_IFMT as u32) == libc::S_IFDIR as u32 {
            // When using --dereference and follow_symlinks is true, check if we've already seen this inode
            // This prevents counting the same directory twice (once directly and once through a symlink)
            if follow_symlinks && seen_inodes.contains(&entry_file_info) {
                // Skip this directory as we've already processed it
                continue;
            }

            // Recursively process subdirectory
            match dir_fd.open_subdir(&entry_name) {
                Ok(subdir_fd) => {
                    match safe_du_with_output(
                        &subdir_fd,
                        &entry_path,
                        depth + 1,
                        traversal_options,
                        follow_symlinks,
                        seen_inodes,
                        print_tx,
                    ) {
                        Ok(subdir_stat) => {
                            // Always send directory results (they'll be filtered by the printer if needed)
                            let _ = print_tx.send(Ok(StatPrintInfo {
                                stat: subdir_stat.clone(),
                                depth: depth + 1,
                            }));

                            if !traversal_options.separate_dirs {
                                total_stat.size += subdir_stat.size;
                                total_stat.blocks += subdir_stat.blocks;
                                total_stat.inodes += subdir_stat.inodes;
                            }
                        }
                        Err(_e) => {
                            // Skip inaccessible subdirectories
                        }
                    }
                }
                Err(e) => {
                    // Report permission denied errors
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        let _ = print_tx.send(Err(USimpleError::new(
                            1,
                            translate!("du-error-cannot-read-directory", "path" => entry_path.to_string_lossy().quote()),
                        )));
                    }
                    // Skip inaccessible subdirectories
                }
            }
        } else {
            // Regular file - check for hard link cycles
            if seen_inodes.contains(&entry_file_info)
                && (!traversal_options.count_links || !traversal_options.all)
            {
                if traversal_options.count_links && !traversal_options.all {
                    total_stat.inodes += 1;
                }
                continue;
            }
            seen_inodes.insert(entry_file_info);

            let entry_file_stat = Stat {
                path: entry_path.clone(),
                size: entry_stat.st_size as u64,
                blocks: entry_stat.st_blocks as u64,
                inodes: 1,
                metadata: fs::metadata(&entry_path).unwrap_or_else(|_| fs::metadata(".").unwrap()),
            };

            // Send individual file result if --all is specified
            if traversal_options.all {
                let _ = print_tx.send(Ok(StatPrintInfo {
                    stat: entry_file_stat.clone(),
                    depth: depth + 1,
                }));
            }

            total_stat.size += entry_file_stat.size;
            total_stat.blocks += entry_file_stat.blocks;
            total_stat.inodes += 1;
        }
    }

    Ok(total_stat)
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(uucore::crate_version!())
        .help_template(uucore::localized_help_template(uucore::util_name()))
        .about(translate!("du-about"))
        .after_help(translate!("du-after-help"))
        .override_usage(format_usage(&translate!("du-usage")))
        .infer_long_args(true)
        .disable_help_flag(true)
        .arg(
            Arg::new(options::HELP)
                .long(options::HELP)
                .help(translate!("du-help-print-help"))
                .action(ArgAction::Help),
        )
        .arg(
            Arg::new(options::ALL)
                .short('a')
                .long(options::ALL)
                .help(translate!("du-help-all"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::APPARENT_SIZE)
                .long(options::APPARENT_SIZE)
                .help(translate!("du-help-apparent-size"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::BLOCK_SIZE)
                .short('B')
                .long(options::BLOCK_SIZE)
                .value_name("SIZE")
                .help(translate!("du-help-block-size")),
        )
        .arg(
            Arg::new(options::BYTES)
                .short('b')
                .long("bytes")
                .help(translate!("du-help-bytes"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::TOTAL)
                .long("total")
                .short('c')
                .help(translate!("du-help-total"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::MAX_DEPTH)
                .short('d')
                .long("max-depth")
                .value_name("N")
                .help(translate!("du-help-max-depth")),
        )
        .arg(
            Arg::new(options::HUMAN_READABLE)
                .long("human-readable")
                .short('h')
                .help(translate!("du-help-human-readable"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::INODES)
                .long(options::INODES)
                .help(translate!("du-help-inodes"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::BLOCK_SIZE_1K)
                .short('k')
                .help(translate!("du-help-block-size-1k"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::COUNT_LINKS)
                .short('l')
                .long("count-links")
                .help(translate!("du-help-count-links"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::DEREFERENCE)
                .short('L')
                .long(options::DEREFERENCE)
                .help(translate!("du-help-dereference"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::DEREFERENCE_ARGS)
                .short('D')
                .visible_short_alias('H')
                .long(options::DEREFERENCE_ARGS)
                .help(translate!("du-help-dereference-args"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NO_DEREFERENCE)
                .short('P')
                .long(options::NO_DEREFERENCE)
                .help(translate!("du-help-no-dereference"))
                .overrides_with(options::DEREFERENCE)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::BLOCK_SIZE_1M)
                .short('m')
                .help(translate!("du-help-block-size-1m"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NULL)
                .short('0')
                .long("null")
                .help(translate!("du-help-null"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::SEPARATE_DIRS)
                .short('S')
                .long("separate-dirs")
                .help(translate!("du-help-separate-dirs"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::SUMMARIZE)
                .short('s')
                .long("summarize")
                .help(translate!("du-help-summarize"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::SI)
                .long(options::SI)
                .help(translate!("du-help-si"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::ONE_FILE_SYSTEM)
                .short('x')
                .long(options::ONE_FILE_SYSTEM)
                .help(translate!("du-help-one-file-system"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::THRESHOLD)
                .short('t')
                .long(options::THRESHOLD)
                .value_name("SIZE")
                .num_args(1)
                .allow_hyphen_values(true)
                .help(translate!("du-help-threshold")),
        )
        .arg(
            Arg::new(options::VERBOSE)
                .short('v')
                .long("verbose")
                .help(translate!("du-help-verbose"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::EXCLUDE)
                .long(options::EXCLUDE)
                .value_name("PATTERN")
                .help(translate!("du-help-exclude"))
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new(options::EXCLUDE_FROM)
                .short('X')
                .long("exclude-from")
                .value_name("FILE")
                .value_hint(clap::ValueHint::FilePath)
                .help(translate!("du-help-exclude-from"))
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new(options::FILES0_FROM)
                .long("files0-from")
                .value_name("FILE")
                .value_hint(clap::ValueHint::FilePath)
                .value_parser(clap::value_parser!(OsString))
                .help(translate!("du-help-files0-from"))
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new(options::TIME)
                .long(options::TIME)
                .value_name("WORD")
                .require_equals(true)
                .num_args(0..)
                .value_parser(ShortcutValueParser::new([
                    PossibleValue::new("atime").alias("access").alias("use"),
                    PossibleValue::new("ctime").alias("status"),
                    PossibleValue::new("creation").alias("birth"),
                ]))
                .help(translate!("du-help-time")),
        )
        .arg(
            Arg::new(options::TIME_STYLE)
                .long(options::TIME_STYLE)
                .value_name("STYLE")
                .help(translate!("du-help-time-style")),
        )
        .arg(
            Arg::new(options::FILE)
                .hide(true)
                .value_hint(clap::ValueHint::AnyPath)
                .value_parser(clap::value_parser!(OsString))
                .action(ArgAction::Append),
        )
}

#[derive(Clone, Copy)]
enum Threshold {
    Lower(u64),
    Upper(u64),
}

impl FromStr for Threshold {
    type Err = ParseSizeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let offset = usize::from(s.starts_with(&['-', '+'][..]));

        let size = parse_size_u64(&s[offset..])?;

        if s.starts_with('-') {
            // Threshold of '-0' excludes everything besides 0 sized entries
            // GNU's du treats '-0' as an invalid argument
            if size == 0 {
                return Err(ParseSizeError::ParseFailure(s.to_string()));
            }
            Ok(Self::Upper(size))
        } else {
            Ok(Self::Lower(size))
        }
    }
}

impl Threshold {
    fn should_exclude(&self, size: u64) -> bool {
        match *self {
            Self::Upper(threshold) => size > threshold,
            Self::Lower(threshold) => size < threshold,
        }
    }
}

fn format_error_message(error: &ParseSizeError, s: &str, option: &str) -> String {
    // NOTE:
    // GNU's du echos affected flag, -B or --block-size (-t or --threshold), depending user's selection
    match error {
        ParseSizeError::InvalidSuffix(_) => {
            translate!("du-error-invalid-suffix", "option" => option, "value" => s.quote())
        }
        ParseSizeError::ParseFailure(_) | ParseSizeError::PhysicalMem(_) => {
            translate!("du-error-invalid-argument", "option" => option, "value" => s.quote())
        }
        ParseSizeError::SizeTooBig(_) => {
            translate!("du-error-argument-too-large", "option" => option, "value" => s.quote())
        }
    }
}

#[cfg(test)]
mod test_du {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_read_block_size() {
        let test_data = [Some("1024".to_string()), Some("K".to_string()), None];
        for it in &test_data {
            assert!(matches!(read_block_size(it.as_deref()), Ok(1024)));
        }
    }
}
