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

use crate::api::vsmtp_plugin_dnsxl;
use rhai::Engine;

#[test]
fn test_building_blocklist() {
    let engine = Engine::new();
    let map = engine.parse_json(
        r#"
            {
                "bl": ["spamhaus", "spamrats"],
            }"#,
        true,
    );
    vsmtp_plugin_dnsxl::blacklist(map.unwrap()).unwrap();
}

#[test]
fn test_building_whitelist() {
    let engine = Engine::new();
    let map = engine.parse_json(
        r#"
            {
                "wl": ["localhost"],
            }"#,
        true,
    );
    vsmtp_plugin_dnsxl::blacklist(map.unwrap()).unwrap();
}

#[test]
fn test_kw_spam_check() {
    let engine = Engine::new();
    let map = engine.parse_json(
        r#"
            {
                "bl": ["s5h"],
            }"#,
        true,
    );
    let mut dnsxl = vsmtp_plugin_dnsxl::blacklist(map.unwrap()).unwrap();
    assert_eq!(
        vsmtp_plugin_dnsxl::contains_bl(&mut dnsxl, "2.0.0.127".into()).type_name(),
        String::from("map")
    );
}

#[test]
fn test_url_spam_check() {
    let engine = Engine::new();
    let map = engine.parse_json(
        r#"
            {
                "bl": ["all.s5h.net"],
            }"#,
        true,
    );
    let mut dnsxl = vsmtp_plugin_dnsxl::blacklist(map.unwrap()).unwrap();
    assert_eq!(
        vsmtp_plugin_dnsxl::contains_bl(&mut dnsxl, "2.0.0.127".into()).type_name(),
        String::from("map")
    );
}

#[test]
fn test_non_spam_check() {
    let engine = Engine::new();
    let map = engine.parse_json(
        r#"
            {
                "bl": ["spamhaus"],
            }"#,
        true,
    );
    let mut dnsxl = vsmtp_plugin_dnsxl::blacklist(map.unwrap()).unwrap();
    assert_eq!(
        vsmtp_plugin_dnsxl::contains_bl(&mut dnsxl, "example.com".into()).type_name(),
        String::from("()")
    );
}

#[test]
fn test_contains_whitelist() {
    let engine = Engine::new();
    let map = engine.parse_json(
        r#"
            {
                "wl": ["localhost"],
            }"#,
        true,
    );
    let mut dnsxl = vsmtp_plugin_dnsxl::whitelist(map.unwrap()).unwrap();
    assert_eq!(
        vsmtp_plugin_dnsxl::contains_wl(&mut dnsxl, "wwW.google.com".into()).type_name(),
        String::from("map")
    );
}
