//  * This file is part of the uutils coreutils package.
//  *
//  * (c) Maciej Dziardziel <fiedzia@gmail.com>
//  *
//  * For the full copyright and license information, please view the LICENSE file
//  * that was distributed with this source code.

// spell-checker:ignore (ToDO) gethostid

extern crate libc;

#[macro_use]
extern crate uucore;

use libc::c_long;

static SYNTAX: &str = "[options]";
static SUMMARY: &str = "";
static LONG_HELP: &str = "";

// currently rust libc interface doesn't include gethostid
extern "C" {
    pub fn gethostid() -> c_long;
}

pub fn uumain(args: impl uucore::Args) -> i32 {
    app!(SYNTAX, SUMMARY, LONG_HELP).parse(args.collect_str());
    hostid();
    0
}

fn hostid() {
    /*
     * POSIX says gethostid returns a "32-bit identifier" but is silent
     * whether it's sign-extended.  Turn off any sign-extension.  This
     * is a no-op unless unsigned int is wider than 32 bits.
     */

    let mut result: c_long;
    unsafe {
        result = gethostid();
    }

    #[allow(overflowing_literals)]
    let mask = 0xffff_ffff;

    result &= mask;
    println!("{:0>8x}", result);
}
