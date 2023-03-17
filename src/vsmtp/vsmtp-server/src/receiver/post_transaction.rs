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
use tokio_stream::StreamExt;
use vqueue::QueueID;
use vsmtp_common::{
    status::{self, Status},
    transfer::{self, error::Rule},
    CodeID, ContextFinished, Reply,
};
use vsmtp_mail_parser::{Mail, MailParser, MessageBody, ParserError, RawBody};
use vsmtp_protocol::{Error, ReceiverContext};
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

        let (queue, should_skip_working, delegated) = match &skipped {
            Some(status @ status::Status::Quarantine(path)) => {
                let quarantine = QueueID::Quarantine { name: path.into() };
                match self.queue_manager.write_ctx(&quarantine, &ctx).await {
                    Ok(()) => (),
                    Err(_e) => return Some(self.reply_in_config(CodeID::Denied)),
                };

                tracing::warn!(status = status.as_ref(), "Rules skipped.");
                (None, None, false)
            }
            Some(status::Status::Delegated(_)) => {
                return Some(self.reply_in_config(CodeID::Denied));
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
                                return Some(self.reply_in_config(CodeID::Denied));
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
            Err(_e) => return Some(self.reply_in_config(CodeID::Denied)),
        };

        if let Some(queue) = queue {
            match self.queue_manager.write_ctx(&queue, &ctx).await {
                Ok(()) => (),
                Err(_e) => {
                    return Some(self.reply_in_config(CodeID::Denied));
                }
            }
        }

        let process = match &should_skip_working {
            Some(false) => &self.working_sender,
            Some(true) => &self.delivery_sender,
            None => return None,
        };

        match process
            .send(ProcessMessage {
                message_uuid,
                delegated,
            })
            .await
        {
            Ok(()) => None,
            Err(_e) => Some(self.reply_in_config(CodeID::Denied)),
        }
    }

    async fn get_message_body(
        &mut self,
        stream: impl tokio_stream::Stream<Item = Result<Vec<u8>, Error>> + Send + Unpin,
    ) -> Result<either::Either<RawBody, Mail>, Reply> {
        tracing::info!("SMTP handshake completed, fetching email...");
        let stream = stream.map(|l| match l {
            Ok(l) => Ok(l),
            Err(Error::Io(io)) => Err(ParserError::Io(io)),
            Err(Error::BufferTooLong { expected, got }) => {
                Err(ParserError::BufferTooLong { expected, got })
            }
            Err(Error::ParsingError(_) | Error::Utf8(_)) => todo!(),
        });

        let mail = match (self.message_parser_factory)().parse(stream).await {
            Ok(mail) => mail,
            Err(ParserError::BufferTooLong { .. }) => {
                return Err(self.reply_in_config(CodeID::MessageSizeExceeded));
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

            let (mail_ctx, message) = std::mem::replace(&mut self.state_internal, None)
                .unwrap()
                .take();
            let mut mail_ctx = mail_ctx
                .unwrap_finished()
                .expect("has been set to finished");

            match status {
                Status::Deny(code_or_reply) => {
                    ctx.deny();
                    Some((self.reply_or_code_in_config(code_or_reply), None))
                }
                Status::Delegated(_) => unreachable!(),
                status => {
                    mail_ctx.connect.skipped = Some(status);
                    //self.on_message_complete(mail_ctx, message).await
                    Some((self.reply_in_config(CodeID::Ok), Some((mail_ctx, message))))
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
                    Status::Deny(code_or_reply) => {
                        ctx.deny();
                        Some((self.reply_or_code_in_config(code_or_reply), None))
                    }
                    Status::Delegated(_) => unreachable!(),
                    status => {
                        mail_ctx.connect.skipped = Some(status);
                        // self.on_message_complete(mail_ctx, message).await
                        Some((self.reply_in_config(CodeID::Ok), Some((mail_ctx, message))))
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
