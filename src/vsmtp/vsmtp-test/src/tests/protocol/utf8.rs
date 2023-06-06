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

use crate::run_test;

use vsmtp_common::addr;
use vsmtp_common::ContextFinished;
use vsmtp_mail_parser::BodyType;
use vsmtp_mail_parser::Mail;
use vsmtp_mail_parser::MailHeaders;
use vsmtp_mail_parser::MessageBody;

macro_rules! test_lang {
    ($lang_code:expr) => {{
        crate::run_test! {
            input = [
                "HELO foobar\r\n",
                "MAIL FROM:<john@doe>\r\n",
                "RCPT TO:<aa@bb>\r\n",
                "DATA\r\n",
                &[
                    "from: john doe <john@doe>\r\n",
                    "subject: ar\r\n",
                    "to: aa@bb\r\n",
                    "message-id: <xxx@localhost.com>\r\n",
                    "date: Tue, 30 Nov 2021 20:54:27 +0100\r\n",
                    "\r\n",
                    &include_str!($lang_code).lines().map(str::to_string).collect::<Vec<_>>().join("\r\n"),
                    // adding a "\r\n" after the mail because [`join`] don t add after the final element
                    "\r\n",
                    ".\r\n",
                ].concat(),
                "QUIT\r\n",
            ],
            expected = [
                "220 testserver.com Service ready\r\n",
                "250 Ok\r\n",
                "250 Ok\r\n",
                "250 Ok\r\n",
                "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
                "250 Ok\r\n",
                "221 Service closing transmission channel\r\n",
            ],
            mail_handler = |ctx: ContextFinished, mut body: MessageBody| {
                assert_eq!(ctx.helo.client_name.to_string(), "foobar");
                assert_eq!(ctx.mail_from.reverse_path, Some(addr!("john@doe")));

                assert!(ctx.rcpt_to.delivery
                    .values()
                    .flatten()
                    .map(|(addr, _)| addr)
                    .cloned()
                    .eq([
                        addr!("aa@bb"),
                    ])
                );

                pretty_assertions::assert_eq!(
                    *body
                        .parsed::<vsmtp_mail_parser::MailMimeParser>()
                        .unwrap(),
                    Mail {
                        headers: MailHeaders([
                            ("from", "john doe <john@doe>"),
                            ("subject", "ar"),
                            ("to", "aa@bb"),
                            ("message-id", "<xxx@localhost.com>"),
                            ("date", "Tue, 30 Nov 2021 20:54:27 +0100"),
                        ]
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect::<Vec<_>>()),
                        body: BodyType::Regular(
                            include_str!($lang_code)
                                .lines()
                                .map(str::to_string)
                                .map(|s| if s.starts_with("..") {
                                    s[1..].to_string()
                                } else {
                                    s
                                })
                                .collect::<Vec<_>>()
                        )
                    }
                );
            },
        }
    }};
}

#[tokio::test]
async fn test_receiver_utf8_zh() {
    test_lang!("../../template/mail/zh.txt");
}

#[tokio::test]
async fn test_receiver_utf8_el() {
    test_lang!("../../template/mail/el.txt");
}

#[tokio::test]
async fn test_receiver_utf8_ar() {
    test_lang!("../../template/mail/ar.txt");
}

#[tokio::test]
async fn test_receiver_utf8_ko() {
    test_lang!("../../template/mail/ko.txt");
}

run_test! {
    fn utf8_in_command,
    input = [
        "EHLO القيام\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "501 Syntax error in parameters or arguments\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn ehlo_with_utf8,
    input = [
        "EHLO foo.bar\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250 SIZE 20000000\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn mail_from_utf8,
    input = [
        "EHLO foobar\r\n",
        "MAIL FROM:<χρήστης@παράδειγμα.ελ> SMTPUTF8\r\n",
        "RCPT TO:<ಬೆಂಬಲ@ಡೇಟಾಮೇಲ್.ಭಾರತ>\r\n",
        "RCPT TO:<用户@例子.广告>\r\n",
        "DATA\r\n",
        concat!(
            "Date: 0\r\n",
            "From: χρήστης@παράδειγμα.ελ\r\n",
            "To: ಬೆಂಬಲ@ಡೇಟಾಮೇಲ್.ಭಾರತ, 用户@例子.广告 \r\n",
            "Subject: ಅಚ್ಚರಿಯ ವಿಷಯ\r\n",
            ".\r\n",
        ),
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250 SIZE 20000000\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn mail_missing_smtputf8,
    input = [
        "EHLO foobar\r\n",
        "MAIL FROM:<χρήστης@παράδειγμα.ελ>\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250 SIZE 20000000\r\n",
        "550 mailbox unavailable\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn rcpt_missing_smtputf8,
    input = [
        "EHLO foobar\r\n",
        "MAIL FROM:<john.doe@mail.com>\r\n",
        "RCPT TO:<用户@例子.广告>\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250 SIZE 20000000\r\n",
        "250 Ok\r\n",
        "553 mailbox name not allowed\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn rcpt_with_smtputf8,
    input = [
        "EHLO foobar\r\n",
        "MAIL FROM:<john.doe@mail.com> SMTPUTF8\r\n",
        "RCPT TO:<用户@例子.广告>\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250 SIZE 20000000\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn data_with_utf8_headers,
    input = [
        "EHLO foobar\r\n",
        "MAIL FROM:<john.doe@mail.com> SMTPUTF8\r\n",
        "RCPT TO:<jenny.doe@mail.com>\r\n",
        "DATA\r\n",
        concat!(
            "Date: 0\r\n",
            "From: john.doe@mail.com\r\n",
            "To: jenny.doe@mail.com\r\n",
            "Subject: IMPORTANT\r\n",
            "custom-header: ליידיק\r\n",
            ".\r\n",
        ),
        "QUIT\r\n",
    ],
    expected = [
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
}
