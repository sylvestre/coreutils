// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore euid egid

//! `--output-format=json`: the ids the text output prints, keyed like jc's
//! `id` parser. Each id comes with its name, null when it has none.

use uucore::entries;
use uucore::json::{Map, Value, json};

use crate::State;

/// The record for one user: `-u`, `-g` and `-G` keep only `uid`, `gid` or
/// `groups`; the default format has them all, plus `euid`/`egid` when they
/// differ and the security context, as the text does.
pub(crate) fn record(state: &State, groups: &[u32], default_format: bool) -> Value {
    let Some(ids) = &state.ids else {
        return Value::Null;
    };
    let mut record = Map::new();
    if state.uflag || default_format {
        record.insert("uid".into(), user(ids.uid));
    }
    if state.gflag || default_format {
        record.insert("gid".into(), group(ids.gid));
    }
    if default_format && !state.user_specified {
        if ids.euid != ids.uid {
            record.insert("euid".into(), user(ids.euid));
        }
        if ids.egid != ids.gid {
            record.insert("egid".into(), group(ids.egid));
        }
    }
    if state.gsflag || default_format {
        let groups = groups.iter().map(|&gid| group(gid)).collect();
        record.insert("groups".into(), Value::Array(groups));
    }
    if default_format
        && !state.user_specified
        && std::env::var_os("POSIXLY_CORRECT").is_none()
        && let Some(Ok(context)) = security_context(state)
    {
        record.insert("context".into(), context_record(&context));
    }
    Value::Object(record)
}

fn user(uid: u32) -> Value {
    json!({ "id": uid, "name": entries::uid2usr(uid).ok() })
}

fn group(gid: u32) -> Value {
    json!({ "id": gid, "name": entries::gid2grp(gid).ok() })
}

/// The security context of the process: `None` without SELinux or SMACK
/// support, `Some(Err(()))` when it can't be read.
pub(crate) fn security_context(state: &State) -> Option<Result<String, ()>> {
    #[cfg(all(feature = "selinux", any(target_os = "linux", target_os = "android")))]
    if state.selinux_supported {
        return Some(
            selinux::SecurityContext::current(false)
                .map(|context| String::from_utf8_lossy(context.as_bytes()).into_owned())
                .map_err(|_| ()),
        );
    }
    #[cfg(all(feature = "smack", target_os = "linux"))]
    if state.smack_supported {
        return Some(uucore::smack::get_smack_label_for_self().map_err(|_| ()));
    }
    let _ = state;
    None
}

/// An SELinux context `user:role:type:level` split in its parts (the level
/// may itself contain colons). A context with fewer parts, like a SMACK
/// label, fills the first keys only.
pub(crate) fn context_record(context: &str) -> Value {
    let keys = ["user", "role", "type", "level"];
    let record: Map<String, Value> = keys
        .iter()
        .zip(context.splitn(keys.len(), ':'))
        .map(|(key, part)| ((*key).to_string(), json!(part)))
        .collect();
    Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::context_record;
    use uucore::json::json;

    #[test]
    fn test_selinux_context_keeps_colons_in_level() {
        assert_eq!(
            context_record("staff_u:sysadm_r:shell_t:s1-s2:c3,c7"),
            json!({
                "user": "staff_u",
                "role": "sysadm_r",
                "type": "shell_t",
                "level": "s1-s2:c3,c7",
            })
        );
    }

    #[test]
    fn test_label_without_colons() {
        assert_eq!(context_record("Floor7"), json!({ "user": "Floor7" }));
    }
}
