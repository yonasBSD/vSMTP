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
use vsmtp_common::mail_context::Finished;
use vsmtp_common::mail_context::{Empty, MailContext, TransactionType};
use vsmtp_config::Config;
use vsmtp_mail_parser::MessageBody;

/// find a file in root examples.
#[macro_export]
macro_rules! root_example {
    ( $( $x:expr ),* ) => {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples/")
            .join(std::path::PathBuf::from_iter([ $( $x, )* ]))
            .to_path_buf()
    };
}

/// Get a config for local test
///
/// # Panics
///
/// * config cannot be built
#[must_use]
pub fn local_test() -> Config {
    Config::builder()
        .with_version_str("<1.0.0")
        .unwrap()
        .without_path()
        .with_server_name("testserver.com")
        .with_user_group_and_default_system("root", "root")
        .unwrap()
        .with_ipv4_localhost()
        .with_default_logs_settings()
        .with_spool_dir_and_default_queues("./tmp/spool")
        .without_tls_support()
        .with_default_smtp_options()
        .with_default_smtp_error_handler()
        .with_default_smtp_codes()
        .without_auth()
        .with_app_at_location("./tmp/app")
        .with_vsl(format!(
            "{}/src/tests/ignore_vsl",
            env!("CARGO_MANIFEST_DIR")
        ))
        .with_default_app_logs()
        .with_system_dns()
        .without_virtual_entries()
        .validate()
        .unwrap()
}

///
#[must_use]
pub fn local_ctx() -> MailContext<Finished> {
    MailContext::<Empty>::connect(
        "127.0.0.1:25".parse().expect(""),
        "127.0.0.1:5977".parse().expect(""),
        "testserver.com".to_string(),
    )
    .helo("client.testserver.com".to_string())
    .mail_from("client@client.testserver.com".parse().expect(""), false)
    .rcpt_to(vec![], TransactionType::Incoming(None))
    .finish()
}

///
#[must_use]
pub fn local_msg() -> MessageBody {
    MessageBody::new(
        [
            "From: NoBody <nobody@domain.tld>",
            "Reply-To: Yuin <yuin@domain.tld>",
            "To: Hei <hei@domain.tld>",
            "Subject: Happy new year",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        "Be happy!\r\n".to_string(),
    )
}
