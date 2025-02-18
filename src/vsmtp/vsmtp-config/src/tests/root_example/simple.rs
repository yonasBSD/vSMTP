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
use crate::Config;

#[test]
fn parse() {
    let path_to_config = std::path::PathBuf::from_iter([
        env!("CARGO_MANIFEST_DIR"),
        "../../../examples/config/simple.vsl",
    ]);

    pretty_assertions::assert_eq!(
        Config::from_vsl_file(&path_to_config).unwrap(),
        Config::builder()
            .with_version_str(&format!(">={}, <3.0.0", env!("CARGO_PKG_VERSION")))
            .unwrap()
            .with_path(path_to_config)
            .with_server_name("my.fqdn.com".parse::<vsmtp_common::Domain>().unwrap())
            .with_user_group_and_default_system("root", "root")
            .unwrap()
            .with_interfaces(
                &["127.0.0.1:25".parse().unwrap()],
                &["127.0.0.1:587".parse().unwrap()],
                &["127.0.0.1:465".parse().unwrap()]
            )
            .with_default_logs_settings()
            .with_default_delivery()
            .without_tls_support()
            .with_default_smtp_options()
            .with_default_smtp_error_handler()
            .with_default_extensions()
            .with_default_app()
            .with_default_vsl_settings()
            .with_default_app_logs()
            .with_system_dns()
            .without_virtual_entries()
            .validate()
    );
}
