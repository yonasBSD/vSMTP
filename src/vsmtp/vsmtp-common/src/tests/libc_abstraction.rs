/*
 * vSMTP mail transfer agent
 * Copyright (C) 2022 viridIT SAS
 *
 * This program is free software: you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program. If not, see https://www.gnu.org/licenses/.
 *
*/
use crate::libc_abstraction::{chown, if_indextoname, if_nametoindex, setgid, setuid};

#[test]
fn test_setuid_current() {
    setuid(users::get_current_uid()).unwrap();
}

#[test]
fn test_setuid_root() {
    setuid(users::get_user_by_name("root").unwrap().uid()).unwrap_err();
}

#[test]
fn test_setgid_current() {
    setgid(users::get_current_gid()).unwrap();
}

#[test]
fn test_setgid_root() {
    setgid(users::get_user_by_name("root").unwrap().primary_group_id()).unwrap_err();
}

#[test]
fn test_if_indextoname() {
    if_indextoname(1).unwrap();
    if_indextoname(0).unwrap_err();
    if_indextoname(1_000_000).unwrap_err();
}

#[test]
fn test_if_nametoindex() {
    if_nametoindex("no_interface_named_like_that").unwrap_err();
    if_nametoindex("no_interface_\0named_like_that").unwrap_err();
    if_nametoindex(&if_indextoname(1).unwrap()).unwrap();
}

#[test]
fn test_chown_file() {
    let user = users::get_user_by_uid(users::get_current_uid()).unwrap();

    assert!(chown(
        std::path::Path::new("./no_such_file_exist"),
        Some(user.uid()),
        None
    )
    .is_err());

    let file_to_create = "./toto";
    let _file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(file_to_create)
        .unwrap();

    chown(std::path::Path::new(file_to_create), Some(user.uid()), None).unwrap();

    std::fs::remove_file(file_to_create).unwrap();
}
