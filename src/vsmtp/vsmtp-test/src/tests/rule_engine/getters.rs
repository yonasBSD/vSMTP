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

use crate::{run_test, tests::auth::unsafe_auth_config};
use vqueue::GenericQueueManager;
use vsmtp_common::addr;

run_test! {
    fn getters,
    input = concat!(
        "EHLO foo\r\n",
        "AUTH ANONYMOUS dG9rZW5fYWJjZGVm\r\n",
        "MAIL FROM:<replace@example.com>\r\n",
        "RCPT TO:<test@example.com>\r\n",
        "DATA\r\n",
        ".\r\n",
        "QUIT\r\n"
    ),
    expected = concat!(
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-AUTH PLAIN LOGIN CRAM-MD5 ANONYMOUS\r\n",
        "250-STARTTLS\r\n",
        "250-8BITMIME\r\n",
        "250 SMTPUTF8\r\n",
        "235 2.7.0 Authentication succeeded\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n"
    ),
    config = unsafe_auth_config(),,mail_handler = {
        #[derive(Default)]
        struct MailHandler;

        #[async_trait::async_trait]
        impl vsmtp_server::OnMail for MailHandler {
            async fn on_mail<
                S: tokio::io::AsyncWrite + tokio::io::AsyncRead + Send + Unpin + std::fmt::Debug,
            >(
                &mut self,
                _: &mut  vsmtp_server::Connection<S>,
                ctx: Box<vsmtp_common::mail_context::MailContext<vsmtp_common::mail_context::Finished>>,
                _:  vsmtp_mail_parser::MessageBody,
                _: std::sync::Arc<dyn GenericQueueManager>,
            ) -> vsmtp_common::CodeID {

                assert_eq!(
                    "john.doe@example.com",
                    ctx.reverse_path().full()
                );

                assert_eq!(
                    vec![
                        addr!("test@example.com"),
                        addr!("add4@example.com"),
                        addr!("replace4@example.com"),
                    ],
                    *ctx.forward_paths().iter()
                        .map(|i| i.address.clone())
                        .collect::<Vec<_>>()
                );

                vsmtp_common::CodeID::Ok
            }
        }

        MailHandler
    },
    rule_script = include_str!("getters.vsl"),
}
