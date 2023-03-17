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

const INNER_DOMAIN: &str = "inner.com";
const OUTER_DOMAIN: &str = "outer.com";

fn get_rules(builder: vsmtp_rule_engine::Builder) -> vsmtp_rule_engine::SubDomainHierarchy {
    builder
        .add_root_filter_rules(
            r#"#{
                connect: [ action "root connect" || { log("warn", "root connect"); } ],
                helo: [ action "root helo" || { log("warn", "root helo"); } ],
                mail: [ action "root mail" || { log("warn", "root mail"); } ],
                rcpt: [ action "root rcpt" || { log("warn", "root rcpt"); } ],
                preq: [ action "root preq" || { log("warn", "root preq"); } ],
            }"#,
        )
        .unwrap()
        .add_domain_rules(INNER_DOMAIN.parse().unwrap())
        .with_incoming(
            r#"#{
                connect: [ action "incoming connect" || { log("warn", "inner.com incoming connect"); } ],
                helo: [ action "incoming helo" || { log("warn", "inner.com incoming helo"); } ],
                mail: [ action "incoming mail" || { log("warn", "inner.com incoming mail"); } ],
                rcpt: [ action "incoming rcpt" || { log("warn", "inner.com incoming rcpt"); } ],
                preq: [ action "incoming preq" || { log("warn", "inner.com incoming preq"); } ],
            }"#,
        )
        .unwrap()
        .with_outgoing(
            r#"#{
                connect: [ action "outgoing connect" || { log("warn", "inner.com outgoing connect"); } ],
                helo: [ action "outgoing helo" || { log("warn", "inner.com outgoing helo"); } ],
                mail: [ action "outgoing mail" || { log("warn", "inner.com outgoing mail"); } ],
                rcpt: [ action "outgoing rcpt" || { log("warn", "inner.com outgoing rcpt"); } ],
                preq: [ action "outgoing preq" || { log("warn", "inner.com outgoing preq"); } ],
            }"#,
        )
        .unwrap()
        .with_internal(
            r#"#{
              connect: [ action "internal connect" || { log("warn", "inner.com internal connect"); } ],
              helo: [ action "internal helo" || { log("warn", "inner.com internal helo"); } ],
              mail: [ action "internal mail" || { log("warn", "inner.com internal mail"); } ],
              rcpt: [ action "internal rcpt" || { log("warn", "inner.com internal rcpt"); } ],
              preq: [ action "internal preq" || { log("warn", "inner.com internal preq"); } ],
          }"#,
        )
        .unwrap()
        .build()
        .build()
}

#[rstest::fixture]
fn logs(#[default("")] mail_from: &str, #[default("")] rcpt_to: &str) -> &'static [&'static str] {
    match (mail_from, rcpt_to) {
        (INNER_DOMAIN, INNER_DOMAIN) => &[
            "WARN vsmtp_rule_engine::api::logging::logging: root connect",
            "WARN vsmtp_rule_engine::api::logging::logging: root helo",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com outgoing mail",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com internal rcpt",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com internal preq",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com outgoing preq",
        ],
        (INNER_DOMAIN, OUTER_DOMAIN) => &[
            "WARN vsmtp_rule_engine::api::logging::logging: root connect",
            "WARN vsmtp_rule_engine::api::logging::logging: root helo",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com outgoing mail",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com outgoing rcpt",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com outgoing preq",
        ],
        (OUTER_DOMAIN, INNER_DOMAIN) => &[
            "WARN vsmtp_rule_engine::api::logging::logging: root connect",
            "WARN vsmtp_rule_engine::api::logging::logging: root helo",
            "WARN vsmtp_rule_engine::api::logging::logging: root mail",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com incoming rcpt",
            "WARN vsmtp_rule_engine::api::logging::logging: inner.com incoming preq",
        ],
        (OUTER_DOMAIN, OUTER_DOMAIN) => &[
            "WARN vsmtp_rule_engine::api::logging::logging: root connect",
            "WARN vsmtp_rule_engine::api::logging::logging: root helo",
            "WARN vsmtp_rule_engine::api::logging::logging: root mail",
            "WARN vsmtp_rule_engine::api::logging::logging: root rcpt",
            "WARN vsmtp_rule_engine::api::logging::logging: root preq",
        ],
        _ => unimplemented!(),
    }
}

#[rstest::rstest]
fn each(
    #[values(INNER_DOMAIN, OUTER_DOMAIN)] mail_from: &str,
    #[values(INNER_DOMAIN, OUTER_DOMAIN)] rcpt_to: &str,
    #[with(mail_from, rcpt_to)] logs: &[&'static str],
) {
    let filename = uuid::Uuid::new_v4();

    let _x = std::fs::create_dir("./tmp");

    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::sync::Arc::new(
            std::fs::File::create(format!("tmp/{filename}")).unwrap(),
        ))
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            crate::run_test! {
                input = [
                    "EHLO foo\r\n".to_owned(),
                    format!("MAIL FROM:<foo@{mail_from}>\r\n"),
                    format!("RCPT TO:<bar@{rcpt_to}>\r\n"),
                    "DATA\r\n".to_owned(),
                    [
                        "From: john doe <john@doe.com>\r\n",
                        "To: green@foo.net\r\n",
                        "Subject: test email\r\n",
                        "\r\n",
                        "This is a raw email.\r\n",
                        ".\r\n",
                    ].concat(),
                    "QUIT\r\n".to_owned(),
                ],
                expected = [
                    "220 testserver.com Service ready\r\n",
                    "250-testserver.com\r\n",
                    "250-STARTTLS\r\n",
                    "250-8BITMIME\r\n",
                    "250 SMTPUTF8\r\n",
                    "250 Ok\r\n",
                    "250 Ok\r\n",
                    "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
                    "250 Ok\r\n",
                    "221 Service closing transmission channel\r\n",
                ],
                hierarchy_builder = |builder| Ok(get_rules(builder))
            }
        });

    let content = std::fs::read_to_string(format!("tmp/{filename}")).unwrap();
    println!("{content}");

    for log in logs {
        assert!(content.contains(log));
    }
}
