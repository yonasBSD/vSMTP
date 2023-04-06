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

use crate::api::redis;
use rhai::Engine;

// FIXME: Ignoring all tests because they are using a local instance of redis which does
//        not exists in CI environments.
#[allow(unused_imports)]
pub mod test {
    use crate::api::vsmtp_plugin_redis;
    use rhai::{Engine, Variant};

    #[ignore]
    #[test]
    fn test_wrong_url() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:0",
                    "connections": 1,
                    "timeout": "1s",
                }"#,
            true,
        );
        assert!(vsmtp_plugin_redis::connect(map.unwrap()).is_err());
    }

    #[ignore]
    #[test]
    fn test_get_string() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::set(&mut server, "get_string", "value".into()).unwrap();
        assert_eq!(
            vsmtp_plugin_redis::get(&mut server, "get_string")
                .unwrap()
                .type_name(),
            String::default().type_name()
        )
    }

    #[ignore]
    #[test]
    fn test_set() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::set(&mut server, "set", "value".into()).unwrap();
        assert_eq!(
            vsmtp_plugin_redis::get(&mut server, "set").unwrap().to_string(),
            "value"
        );
    }

    #[ignore]
    #[test]
    fn test_append() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                    "timeout": "1s"
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::set(&mut server, "append", "value".into()).unwrap();
        vsmtp_plugin_redis::append(&mut server, "append", " and another value".into()).unwrap();
        assert_eq!(
            vsmtp_plugin_redis::get(&mut server, "append").unwrap().to_string(),
            "value and another value"
        );
    }

    #[ignore]
    #[test]
    fn test_delete() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                    "timeout": "1s"
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::set(&mut server, "delete", "value".into()).unwrap();
        assert_eq!(
            vsmtp_plugin_redis::delete(&mut server, "delete").unwrap(),
            "OK"
        );
    }

    #[ignore]
    #[test]
    #[should_panic]
    fn test_non_existing_delete() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                    "timeout": "1s"
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::delete(&mut server, "delete_2").unwrap();
    }

    #[ignore]
    #[test]
    fn test_increment() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::set(&mut server, "increment", rhai::Dynamic::from_int(1)).unwrap();
        vsmtp_plugin_redis::increment(&mut server, "increment", 10).unwrap();
        assert_eq!(
            vsmtp_plugin_redis::get(&mut server, "increment").unwrap().to_string(),
            "11"
        );
    }

    #[ignore]
    #[test]
    fn test_decrement() {
        let engine = Engine::new();
        let map = engine.parse_json(
            r#"
                {
                    "url": "redis://localhost:6379",
                    "connections": 1,
                    "timeout": "1s"
                }"#,
            true,
        );
        let mut server = vsmtp_plugin_redis::connect(map.unwrap()).unwrap();
        vsmtp_plugin_redis::set(&mut server, "decrement", rhai::Dynamic::from_int(10)).unwrap();
        vsmtp_plugin_redis::decrement(&mut server, "decrement", 1).unwrap();
        assert_eq!(
            vsmtp_plugin_redis::get(&mut server, "decrement").unwrap().to_string(),
            "9"
        );
    }
}
