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
use vsmtp_common::ContextFinished;
use vsmtp_mail_parser::MessageBody;

const CONFIG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../examples/family/vsmtp.vsl"
);

run_test! {
    fn test_family_setup_1,
    input = [
        "HELO example.com\r\n",
        "MAIL FROM:<a@spam-domain.org>\r\n",
    ],
    expected = [
        "220 doe-family.com Service ready\r\n",
        "250 Ok\r\n",
        "451 4.7.1 Sender is not authorized. Please try again.\r\n",
    ],
    config = vsmtp_config::Config::from_vsl_file(CONFIG).unwrap()
}

run_test! {
    fn test_family_setup_2,
    input = [
        "HELO example.com\r\n",
        "MAIL FROM:<a@example.com>\r\n",
        "RCPT TO:<jenny.doe@doe-family.com>\r\n",
        "RCPT TO:<somebody.else@doe-family.com>\r\n",
        "DATA\r\n",
        concat!(
            "Date: Wed, 6 Dec 2000 05:55:00 -0800 (PST)\r\n",
            "From: a@example.com\r\n",
            "To: jenny.doe@doe-family.com, somebody.else@doe-family.com\r\n",
            "Subject: Hi from France!\r\n",
            "\r\n",
            "Hey Jenny ! It's been a while since ....\r\n",
            ".\r\n",
        ),
        "QUIT\r\n"
    ],
    expected = [
        "220 doe-family.com Service ready\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n"
    ],
    config = vsmtp_config::Config::from_vsl_file(CONFIG).unwrap(),
    mail_handler = |ctx: ContextFinished, _: MessageBody| {
        ctx.rcpt_to.delivery
            .values()
            .flatten()
            .find(|(addr, _)| *addr == "jane.doe@doe-family.com".parse().unwrap())
            .unwrap();
    },
}
