// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) getlogin userlogin

use clap::Command;
use std::ffi::CStr;
use uucore::init_clap_with_l10n;
use uucore::locale::get_message;
use uucore::{error::UResult, show_error};

fn get_userlogin() -> Option<String> {
    unsafe {
        let login: *const libc::c_char = libc::getlogin();
        if login.is_null() {
            None
        } else {
            Some(String::from_utf8_lossy(CStr::from_ptr(login).to_bytes()).to_string())
        }
    }
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let _ = init_clap_with_l10n!(uu_app().try_get_matches_from(args));

    match get_userlogin() {
        Some(userlogin) => println!("{userlogin}"),
        None => show_error!("{}", get_message("logname-error-no-login-name")),
    }

    Ok(())
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(uucore::crate_version!())
        .override_usage(uucore::util_name())
        .about(get_message("logname-about"))
        .infer_long_args(true)
}
