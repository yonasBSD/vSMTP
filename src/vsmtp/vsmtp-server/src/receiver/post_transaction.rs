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

use crate::{Handler, ProcessMessage};
use futures_util::TryStreamExt;
use vqueue::QueueID;
use vsmtp_common::{
    status::{self, Status},
    transfer::{self, error::Rule},
    ContextFinished, Reply,
};
use vsmtp_mail_parser::{Mail, MailParser, MessageBody, ParserError, RawBody};
use vsmtp_protocol::{Error, ParseArgsError, ReceiverContext};
use vsmtp_rule_engine::{ExecutionStage, RuleEngine, RuleState};

impl<Parser, ParserFactory> Handler<Parser, ParserFactory>
where
    Parser: MailParser + Send + Sync,
    ParserFactory: Fn() -> Parser + Send + Sync,
{
    pub(super) fn handle_preq_header(
        rule_engine: &RuleEngine,
        state: &RuleState,
        mut skipped: Option<Status>,
        mut mail: either::Either<RawBody, Mail>,
    ) -> Status {
        // NOTE: some header might has been added by the user
        // before the reception of the message
        {
            let message = state.message();
            let mut guard = message.write().expect("message poisoned");

            let iter = guard.inner().headers_lines();
            match &mut mail {
                either::Left(raw) => raw.prepend_header(iter.map(str::to_owned)),
                either::Right(parsed) => {
                    parsed.prepend_headers(iter.filter_map(|s| {
                        s.split_once(':')
                            .map(|(key, value)| (key.to_string(), value.to_string()))
                    }));
                }
            };
            *guard = MessageBody::from(mail);
        }

        state
            .context()
            .write()
            .expect("state poisoned")
            .to_finished()
            .expect("bad state");

        let status = rule_engine.run_when(state, &mut skipped, ExecutionStage::PreQ);

        if let Some(skipped) = skipped {
            state
                .context()
                .write()
                .expect("state poisoned")
                .set_skipped(skipped);
        }
        status
    }

    // TODO: enhance error handling
    pub(super) async fn on_message_completed_inner(
        &self,
        mut ctx: ContextFinished,
        msg: MessageBody,
    ) -> Option<Reply> {
        let (mut message_uuid, skipped) = (ctx.mail_from.message_uuid, ctx.connect.skipped.clone());

        let denied = "554 permanent problems with the remote server\r\n"
            .parse::<Reply>()
            .unwrap();

        let (queue, should_skip_working, delegated) = match &skipped {
            Some(status @ status::Status::Quarantine(path)) => {
                let quarantine = QueueID::Quarantine { name: path.into() };
                match self.queue_manager.write_ctx(&quarantine, &ctx).await {
                    Ok(()) => (),
                    Err(_e) => return Some(denied),
                };

                tracing::warn!(status = status.as_ref(), "Rules skipped.");
                (None, None, false)
            }
            Some(status::Status::Delegated(_)) => {
                return Some(denied);
            }
            Some(status::Status::DelegationResult) => {
                if let Some(old_message_id) =
                    msg.get_header("X-VSMTP-DELEGATION").and_then(|header| {
                        vsmtp_mail_parser::get_mime_header("X-VSMTP-DELEGATION", &header)
                            .args
                            .get("id")
                            .cloned()
                    })
                {
                    message_uuid =
                        match <uuid::Uuid as std::str::FromStr>::from_str(&old_message_id) {
                            Ok(uuid) => uuid,
                            Err(_e) => {
                                return Some(denied);
                            }
                        }
                }

                (None, Some(false), true)
            }
            Some(status::Status::Deny(code)) => {
                for rcpt in &mut ctx.rcpt_to.delivery.values_mut().flatten() {
                    rcpt.1 = transfer::Status::failed(Rule::Denied(code.clone()));
                }

                (Some(QueueID::Dead), None, false)
            }
            None | Some(status::Status::Next) => (Some(QueueID::Working), Some(false), false),
            Some(reason) => {
                tracing::warn!(stage = %ExecutionStage::PreQ, status = ?reason.as_ref(), "Rules skipped.");
                (Some(QueueID::Deliver), Some(true), false)
            }
        };

        match self.queue_manager.write_msg(&message_uuid, &msg).await {
            Ok(()) => (),
            Err(_e) => return Some(denied),
        };

        if let Some(queue) = queue {
            match self.queue_manager.write_ctx(&queue, &ctx).await {
                Ok(()) => (),
                Err(_e) => {
                    return Some(denied);
                }
            }
        }

        let process_msg = if delegated {
            ProcessMessage::delegated
        } else {
            ProcessMessage::new
        }(message_uuid);

        let process = match &should_skip_working {
            Some(false) => self.emitter.send_to_working(process_msg).await,
            Some(true) => self.emitter.send_to_delivery(process_msg).await,
            None => Ok(()),
        };

        match process {
            Ok(()) => None,
            Err(_e) => Some(denied),
        }
    }

    fn convert_error(e: Error) -> ParserError {
        if e.get_ref().is_some() {
            match e.into_inner().unwrap().downcast::<std::io::Error>() {
                Ok(io) => ParserError::Io(*io),
                Err(otherwise) => match otherwise.downcast::<ParseArgsError>().map(|i| *i) {
                    Ok(ParseArgsError::BufferTooLong { expected, got }) => {
                        ParserError::BufferTooLong { expected, got }
                    }
                    Ok(otherwise) => ParserError::InvalidMail(otherwise.to_string()),
                    Err(otherwise) => ParserError::InvalidMail(otherwise.to_string()),
                },
            }
        } else {
            ParserError::InvalidMail(e.to_string())
        }
    }

    async fn get_message_body(
        &mut self,
        stream: impl tokio_stream::Stream<Item = Result<Vec<u8>, Error>> + Send + Unpin,
    ) -> Result<either::Either<RawBody, Mail>, Reply> {
        tracing::info!("SMTP handshake completed, fetching email...");
        let stream = stream.map_err(Self::convert_error);

        let mail = match (self.message_parser_factory)()
            .parse(stream, self.config.server.esmtp.size)
            .await
        {
            Ok(mail) => mail,
            Err(ParserError::BufferTooLong { .. }) => {
                return Err(
                    "552 4.3.1 Message size exceeds fixed maximum message size\r\n"
                        .parse::<Reply>()
                        .unwrap(),
                );
            }
            Err(ParserError::MailSizeExceeded { .. }) => {
                return Err(
                    "552 4.3.1 Message size exceeds fixed maximum message size\r\n"
                        .parse::<Reply>()
                        .unwrap(),
                )
            }

            Err(otherwise) => todo!("handle error cleanly {:?}", otherwise),
        };

        tracing::info!("Message body fully received, processing...");
        Ok(mail)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) async fn on_message_inner(
        &mut self,
        ctx: &mut ReceiverContext,
        stream: impl tokio_stream::Stream<Item = Result<Vec<u8>, Error>> + Send + Unpin,
    ) -> (Reply, Option<Vec<(ContextFinished, MessageBody)>>) {
        let mail = match self.get_message_body(stream).await {
            Ok(mail) => mail,
            Err(reply) => return (reply, None),
        };

        let internal_reply = if let Some(state_internal) = &self.state_internal {
            let status = Self::handle_preq_header(
                &self.rule_engine,
                state_internal,
                self.skipped.clone(),
                mail.clone(),
            );

            let (mail_ctx, message) = self.state_internal.take().unwrap().take();
            let mut mail_ctx = mail_ctx
                .unwrap_finished()
                .expect("has been set to finished");

            match status {
                Status::Deny(reply) => {
                    ctx.deny();
                    Some((reply, None))
                }
                Status::Delegated(_) => unreachable!(),
                status => {
                    mail_ctx.connect.skipped = Some(status);
                    Some((
                        "250 Ok\r\n".parse::<Reply>().unwrap(),
                        Some((mail_ctx, message)),
                    ))
                }
            }
        } else {
            None
        };
        let reply = {
            let status = Self::handle_preq_header(
                &self.rule_engine,
                &self.state,
                self.skipped.clone(),
                mail,
            );
            let (client_addr, server_addr, server_name, timestamp, uuid) = {
                let ctx = self.state.context();
                let ctx = ctx.read().expect("state poisoned");
                (
                    *ctx.client_addr(),
                    *ctx.server_addr(),
                    ctx.server_name().clone(),
                    *ctx.connection_timestamp(),
                    *ctx.connection_uuid(),
                )
            };
            let (mail_ctx, message) = std::mem::replace(
                &mut self.state,
                self.rule_engine.spawn_at_connect(
                    client_addr,
                    server_addr,
                    server_name,
                    timestamp,
                    uuid,
                ),
            )
            .take();
            let mut mail_ctx = mail_ctx
                .unwrap_finished()
                .expect("has been set to finished");

            self.state
                .context()
                .write()
                .expect("state poisoned")
                .to_helo(
                    mail_ctx.helo.client_name.clone(),
                    mail_ctx.helo.using_deprecated,
                )
                .expect("bad state");

            if mail_ctx.rcpt_to.delivery.is_empty() {
                None
            } else {
                match status {
                    Status::Deny(reply) => {
                        ctx.deny();
                        Some((reply, None))
                    }
                    Status::Delegated(_) => unreachable!(),
                    status => {
                        mail_ctx.connect.skipped = Some(status);
                        Some((
                            "250 Ok\r\n".parse::<Reply>().unwrap(),
                            Some((mail_ctx, message)),
                        ))
                    }
                }
            }
        };

        match (internal_reply, reply) {
            (Some((internal_reply, internal)), Some((reply, other))) => (
                internal_reply.extended(&reply),
                Some([internal, other].into_iter().flatten().collect()),
            ),
            (Some((internal_reply, internal)), None) => (internal_reply, internal.map(|i| vec![i])),
            (None, Some((reply, other))) => (reply, other.map(|i| vec![i])),
            // both mail are empty: should be unreachable
            (None, None) => todo!(),
        }
    }
}
