// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore euid egid

//! `--output-format=json`: the ids the text output prints, keyed like jc's
//! `id` parser. Each id comes with its name, null when it has none.

use uucore::entries;
use uucore::json::{Map, Value};

use crate::State;

/// The record for one user: `-u`, `-g` and `-G` keep only `uid`, `gid` or
/// `groups`; the default format has them all, plus `euid`/`egid` when they
/// differ, as the text does.
pub(crate) fn record(state: &State, groups: &[u32], default_format: bool) -> Value {
    let Some(ids) = &state.ids else {
        return Value::Null;
    };
    let mut record = Map::new();
    if state.uflag || default_format {
        record.insert("uid", user(ids.uid));
    }
    if state.gflag || default_format {
        record.insert("gid", group(ids.gid));
    }
    if default_format && !state.user_specified {
        if ids.euid != ids.uid {
            record.insert("euid", user(ids.euid));
        }
        if ids.egid != ids.gid {
            record.insert("egid", group(ids.egid));
        }
    }
    if state.gsflag || default_format {
        let groups = groups.iter().map(|&gid| group(gid)).collect();
        record.insert("groups", Value::Array(groups));
    }
    Value::Object(record)
}

fn user(uid: u32) -> Value {
    Value::object([
        ("id", Value::from(uid)),
        ("name", Value::from(entries::uid2usr(uid).ok())),
    ])
}

fn group(gid: u32) -> Value {
    Value::object([
        ("id", Value::from(gid)),
        ("name", Value::from(entries::gid2grp(gid).ok())),
    ])
}
