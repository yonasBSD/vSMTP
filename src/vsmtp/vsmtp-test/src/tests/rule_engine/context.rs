/*
 * vSMTP mail transfer agent
 * Copyright (C) 2022 viridIT SAS
 *
 * This program is free software: you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or any later version.
 *
 *  This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program. If not, see https://www.gnu.org/licenses/.
 *
*/

use crate::run_test;
use vsmtp_rule_engine::ExecutionStage;

const RULE: &str = r#"
#{
    {stage}: [
        action "get" || {
            {snippet};
        }
    ]
}
"#;

#[rstest::fixture]
fn snipped_info(
    #[default("")] snippet: &str,
) -> (ExecutionStage, &'static str, &'static [&'static str]) {
    match snippet {
        "ctx::connection_timestamp()" => (
            "connect".parse().unwrap(),
            "connection_timestamp",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::client_address()" => (
            "connect".parse().unwrap(),
            "client_address",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::client_ip()" => (
            "connect".parse().unwrap(),
            "client_ip",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::client_port()" => (
            "connect".parse().unwrap(),
            "client_port",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::server_address()" => (
            "connect".parse().unwrap(),
            "server_address",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::server_ip()" => (
            "connect".parse().unwrap(),
            "server_ip",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::server_port()" => (
            "connect".parse().unwrap(),
            "server_port",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::server_name()" => (
            "connect".parse().unwrap(),
            "server_name",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "ctx::is_secured()" => (
            "connect".parse().unwrap(),
            "is_secured",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "auth::is_authenticated()" => (
            "connect".parse().unwrap(),
            "is_authenticated",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        "auth::credentials()" => (
            "connect".parse().unwrap(),
            "credentials",
            &["connect", "helo", "mail", "rcpt", "preq"],
        ),
        // "auth::credentials().anonymous_token" => ("connect".parse().unwrap(), "anonymous_token"),
        // "auth::credentials().authpass" => ("connect".parse().unwrap(), "authpass"),
        // "auth::credentials().authid" => ("connect".parse().unwrap(), "authid"),
        // "auth::credentials().type" => ("connect".parse().unwrap(), "type"),
        "ctx::helo()" => (
            "helo".parse().unwrap(),
            "client_name",
            &["helo", "mail", "rcpt", "preq"],
        ),

        "ctx::mail_from()" => (
            "mail".parse().unwrap(),
            "reverse_path",
            &["mail", "rcpt", "preq"],
        ),
        "ctx::mail_timestamp()" => (
            "mail".parse().unwrap(),
            "mail_timestamp",
            &["mail", "rcpt", "preq"],
        ),
        "ctx::message_id()" => (
            "mail".parse().unwrap(),
            "message_uuid",
            &["mail", "rcpt", "preq"],
        ),

        "ctx::rcpt()" | "ctx::rcpt_list()" => {
            ("rcpt".parse().unwrap(), "forward_paths", &["rcpt", "preq"])
        }

        _ => unimplemented!(),
    }
}

#[allow(unused_variables)]
#[rstest::fixture]
fn expected(
    #[default("")] snippet: &str,
    #[default(ExecutionStage::Connect)] called_at: ExecutionStage,
    #[with(snippet)] snipped_info: (ExecutionStage, &'static str, &'static [&'static str]),
) -> (Vec<&'static str>, Option<String>) {
    let (available_after, props, stages) = snipped_info;

    match dbg!(called_at, available_after) {
        (called_at, available_after) if called_at >= available_after => (
            vec![
                "220 testserver.com Service ready\r\n",
                "250-testserver.com\r\n",
                "250-8BITMIME\r\n",
                "250-SMTPUTF8\r\n",
                "250-STARTTLS\r\n",
                "250-PIPELINING\r\n",
                "250 SIZE 20000000\r\n",
                "250 Ok\r\n",
                "250 Ok\r\n",
                "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
                "250 Ok\r\n",
                "221 Service closing transmission channel\r\n",
            ],
            None,
        ),
        (
            ExecutionStage::Connect,
            ExecutionStage::Helo | ExecutionStage::MailFrom | ExecutionStage::RcptTo,
        ) => (
            vec!["554 permanent problems with the remote server\r\n"],
            Some(format!(
                "vsl execution produced an error: Runtime error: \
                    field '{props}' is available in [{}]",
                stages.join(", ")
            )),
        ),
        (ExecutionStage::Helo, ExecutionStage::MailFrom | ExecutionStage::RcptTo) => (
            vec![
                "220 testserver.com Service ready\r\n",
                "554 permanent problems with the remote server\r\n",
            ],
            Some(format!(
                "vsl execution produced an error: Runtime error: \
                    field '{props}' is available in [{}]",
                stages.join(", ")
            )),
        ),
        (ExecutionStage::MailFrom, ExecutionStage::RcptTo) => (
            vec![
                "220 testserver.com Service ready\r\n",
                "250-testserver.com\r\n",
                "250-8BITMIME\r\n",
                "250-SMTPUTF8\r\n",
                "250-STARTTLS\r\n",
                "250-PIPELINING\r\n",
                "250 SIZE 20000000\r\n",
                "554 permanent problems with the remote server\r\n",
            ],
            Some(format!(
                "vsl execution produced an error: Runtime error: \
                field '{props}' is available in [{}]",
                stages.join(", ")
            )),
        ),
        _ => todo!(),
    }
}

#[rstest::rstest]
// #[test_log::test]
#[trace]
// after connect
#[case("ctx::connection_timestamp()")]
#[case("ctx::client_address()")]
#[case("ctx::client_ip()")]
#[case("ctx::client_port()")]
#[case("ctx::server_address()")]
#[case("ctx::server_ip()")]
#[case("ctx::server_port()")]
#[case("ctx::server_name()")]
#[case("ctx::is_secured()")]
#[case("auth::is_authenticated()")]
// #[case("auth::credentials()")]
// #[case("auth::credentials().anonymous_token")]
// #[case("auth::credentials().authpass")]
// #[case("auth::credentials().authid")]
// #[case("auth::credentials().type")]
// after helo
#[case("ctx::helo()")]
// after mail
#[case("ctx::mail_from()")]
#[case("ctx::mail_timestamp()")]
#[case("ctx::message_id()")]
// after rcpt
#[case("ctx::rcpt()")]
#[case("ctx::rcpt_list()")]
fn each(
    #[values(
        ExecutionStage::Connect,
        ExecutionStage::Helo,
        ExecutionStage::MailFrom,
        ExecutionStage::RcptTo,
        ExecutionStage::PreQ
    )]
    stage: ExecutionStage,
    #[case] snippet: &str,
    #[with(snippet, stage)] expected: (Vec<&'static str>, Option<String>),
) {
    let (expected, logs) = expected;
    let filename = uuid::Uuid::new_v4();

    let _x = std::fs::create_dir("./tmp");

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(std::sync::Arc::new(
            std::fs::File::create(format!("tmp/{filename}")).unwrap(),
        ))
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let rule = RULE
        .replace("{stage}", &stage.to_string())
        .replace("{snippet}", snippet);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            run_test! {
                input = [
                    "EHLO foo\r\n",
                    "MAIL FROM:<john@server.com>\r\n",
                    "RCPT TO:<doe@server.com>\r\n",
                    "DATA\r\n",
                    concat!(
                        "From: john doe <john@doe.com>\r\n",
                        "To: green@foo.net\r\n",
                        "Subject: test email\r\n",
                        "\r\n",
                        "This is a raw email.\r\n",
                        ".\r\n",
                    ),
                    "QUIT\r\n",
                ],
                expected = expected,
                hierarchy_builder = move |builder| {
                    Ok(builder.add_root_filter_rules(&rule).unwrap()
                        .add_domain_rules("server.com".parse().unwrap())
                        .with_incoming(&rule).unwrap()
                        .with_outgoing(&rule).unwrap()
                        .with_internal(&rule).unwrap()
                        .build()
                    .build())
                },
            }
        });

    if let Some(logs) = logs {
        let content = std::fs::read_to_string(format!("tmp/{filename}")).unwrap();
        dbg!(&logs);

        let content = content.lines();
        assert!(content
            .filter(|l| l.contains("ERROR"))
            .any(|l| dbg!(l).contains(&logs)));
    }
}
