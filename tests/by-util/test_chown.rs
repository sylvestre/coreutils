use crate::common::util::*;
use rust_users::*;

extern crate chown;

#[cfg(test)]
mod test_passgrp {
    use super::chown::entries::{gid2grp, grp2gid, uid2usr, usr2uid};

    #[test]
    fn test_usr2uid() {
        assert_eq!(0, usr2uid("root").unwrap());
        assert!(usr2uid("88888888").is_err());
        assert!(usr2uid("auserthatdoesntexist").is_err());
    }

    #[test]
    fn test_grp2gid() {
        if cfg!(target_os = "linux") || cfg!(target_os = "android") || cfg!(target_os = "windows") {
            assert_eq!(0, grp2gid("root").unwrap())
        } else {
            assert_eq!(0, grp2gid("wheel").unwrap());
        }
        assert!(grp2gid("88888888").is_err());
        assert!(grp2gid("agroupthatdoesntexist").is_err());
    }

    #[test]
    fn test_uid2usr() {
        assert_eq!("root", uid2usr(0).unwrap());
        assert!(uid2usr(88888888).is_err());
    }

    #[test]
    fn test_gid2grp() {
        if cfg!(target_os = "linux") || cfg!(target_os = "android") || cfg!(target_os = "windows") {
            assert_eq!("root", gid2grp(0).unwrap());
        } else {
            assert_eq!("wheel", gid2grp(0).unwrap());
        }
        assert!(gid2grp(88888888).is_err());
    }
}

#[test]
fn test_invalid_option() {
    new_ucmd!().arg("-w").arg("-q").arg("/").fails();
}

#[test]
#[cfg(target_os = "linux")]
fn test_big_p() {
    if get_effective_uid() != 0 {
        new_ucmd!()
            .arg("-RP")
            .arg("bin")
            .arg("/proc/self/cwd")
            .fails()
            .stderr_is(
                "chown: changing ownership of '/proc/self/cwd': Operation not permitted (os error 1)\n",
            );
    }
}
