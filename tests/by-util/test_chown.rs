use crate::common::util::*;

extern crate chown;
// pub use self::uu_chown::*;

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
fn test_chown_myself() {
    let scene = TestScenario::new(util_name!());
    let result = scene.cmd("whoami").run();
    println!("results {}", result.stdout);

    let (at, mut ucmd) = at_and_ucmd!();
    let file1 = "test_install_target_dir_file_a1";

    at.touch(file1);
    ucmd.arg(result.stdout.trim_end()).arg(file1).succeeds();
}

#[test]
fn test_chown_myself_second() {
    let scene = TestScenario::new(util_name!());
    let result = scene.cmd("whoami").run();
    println!("results {}", result.stdout);

    let (at, mut ucmd) = at_and_ucmd!();
    let file1 = "test_install_target_dir_file_a1";

    at.touch(file1);
    ucmd.arg(result.stdout.trim_end().to_owned() + ":")
        .arg(file1)
        .succeeds();
}

#[test]
fn test_chown_myself_group() {
    let scene = TestScenario::new(util_name!());
    let result = scene.cmd("whoami").run();
    println!("results {}", result.stdout);

    let (at, mut ucmd) = at_and_ucmd!();
    let file1 = "test_install_target_dir_file_a1";
    let perm = result.stdout.trim_end().to_owned() + ":" + result.stdout.trim_end();
    at.touch(file1);
    ucmd.arg(perm).arg(file1).succeeds();
}

#[test]
fn test_chown_only_group() {
    let scene = TestScenario::new(util_name!());
    let result = scene.cmd("whoami").run();
    println!("results {}", result.stdout);

    let (at, mut ucmd) = at_and_ucmd!();
    let file1 = "test_install_target_dir_file_a1";
    let perm = ":".to_owned() + result.stdout.trim_end();
    at.touch(file1);
    ucmd.arg(perm).arg(file1).succeeds();
}
