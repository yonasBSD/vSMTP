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
use crate::scheduler;

use tokio_rustls::rustls;
use vqueue::GenericQueueManager;
use vsmtp_common::{status::Status, Address, ContextFinished, Reply, Stage, TransactionType};
use vsmtp_config::Config;
use vsmtp_delivery::Deliver;
use vsmtp_mail_parser::{MailParser, MessageBody};
use vsmtp_protocol::{
    AuthArgs, AuthError, CallbackWrap, EhloArgs, Error, HeloArgs, MailFromArgs, RcptToArgs,
    ReceiverContext,
};
use vsmtp_rule_engine::{ExecutionStage, RuleEngine, RuleState};

///
pub struct Handler<Parser, ParserFactory>
where
    Parser: MailParser + Send + Sync,
    ParserFactory: Fn() -> Parser + Send + Sync,
{
    /// Rule engine data used in the current transaction, like the email context, server configuration, etc.
    pub(super) state: std::sync::Arc<RuleState>,
    // NOTE: In case the transaction context is outgoing, we create two states
    //       to run two batches of rules at the same time, one for internal transaction
    //       with recipients that have the same domain as the sender, and another
    //       for any other recipient domain.
    // FIXME: find another way to do this
    pub(super) state_internal: Option<std::sync::Arc<RuleState>>,
    pub(super) skipped: Option<Status>,
    //
    pub(super) config: std::sync::Arc<Config>,
    pub(super) rustls_config: Option<std::sync::Arc<rustls::ServerConfig>>,
    pub(super) rule_engine: std::sync::Arc<RuleEngine>,
    pub(super) queue_manager: std::sync::Arc<dyn GenericQueueManager>,

    pub(super) message_parser_factory: ParserFactory,

    pub(super) emitter: std::sync::Arc<scheduler::Emitter>,
}

#[async_trait::async_trait]
impl<Parser: MailParser + Send + Sync, ParserFactory: Fn() -> Parser + Send + Sync>
    vsmtp_protocol::ReceiverHandler for Handler<Parser, ParserFactory>
{
    type Item = (ContextFinished, MessageBody);

    fn generate_sasl_callback(&self) -> CallbackWrap {
        self.generate_sasl_callback_inner()
    }

    async fn on_post_tls_handshake(
        &mut self,
        sni: Option<String>,
        protocol_version: rustls::ProtocolVersion,
        cipher_suite: rustls::CipherSuite,
        peer_certificates: Option<Vec<rustls::Certificate>>,
        alpn_protocol: Option<Vec<u8>>,
    ) -> Reply {
        self.on_post_tls_handshake_inner(
            sni,
            protocol_version,
            cipher_suite,
            peer_certificates,
            alpn_protocol,
        )
    }

    async fn on_starttls(&mut self, ctx: &mut ReceiverContext) -> Reply {
        self.on_starttls_inner(ctx)
    }

    async fn on_auth(&mut self, ctx: &mut ReceiverContext, args: AuthArgs) -> Option<Reply> {
        self.on_auth_inner(ctx, args)
    }

    async fn on_post_auth(
        &mut self,
        ctx: &mut ReceiverContext,
        result: Result<(), AuthError>,
    ) -> Reply {
        self.on_post_auth_inner(ctx, result)
    }

    async fn on_helo(&mut self, ctx: &mut ReceiverContext, args: HeloArgs) -> Reply {
        self.on_helo_inner(ctx, args)
    }

    async fn on_ehlo(&mut self, ctx: &mut ReceiverContext, args: EhloArgs) -> Reply {
        self.on_ehlo_inner(ctx, args)
    }

    async fn on_mail_from(&mut self, ctx: &mut ReceiverContext, args: MailFromArgs) -> Reply {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .to_mail_from(args.reverse_path, args.use_smtputf8)
            .expect("bad state");

        match self
            .rule_engine
            .run_when(&self.state, &mut self.skipped, ExecutionStage::MailFrom)
        {
            Status::Faccept(reply) | Status::Accept(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                "250 Ok\r\n".parse::<Reply>().unwrap()
            }
            // on the mail from stage, reject acts as a deny.
            Status::Reject(reply) | Status::Deny(reply) => {
                ctx.deny();
                reply
            }
            Status::Delegated(_) => unreachable!(),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn on_rcpt_to(&mut self, ctx: &mut ReceiverContext, args: RcptToArgs) -> Reply {
        {
            // FIXME: handle internal state too ??
            let locked_context = self.state.context();
            let context = locked_context.read().expect("state poisoned");
            if context.forward_paths().map_or(0, Vec::len) >= self.config.server.smtp.rcpt_count_max
            {
                return "452 Requested action not taken: too many recipients\r\n"
                    .parse::<Reply>()
                    .unwrap();
            } else if !context.is_utf8_advertised() && !args.forward_path.full().is_ascii() {
                return "553 mailbox name not allowed\r\n".parse::<Reply>().unwrap();
            }
        }

        let is_internal = {
            let ctx = self.state.context();
            let mut ctx = ctx.write().expect("state poisoned");
            let reverse_path = ctx.reverse_path().expect("bad state").clone();
            let reverse_path_domain = reverse_path.as_ref().map(Address::domain);

            let (is_outgoing, is_handled) = (
                reverse_path.as_ref().map_or(false, |reverse_path| {
                    self.rule_engine.is_handled_domain(&reverse_path.domain())
                }),
                self.rule_engine
                    .is_handled_domain(&args.forward_path.domain()),
            );

            match (is_outgoing, is_handled) {
                (true, true) if Some(args.forward_path.domain()) == reverse_path_domain => {
                    tracing::debug!(
                        "INTERNAL: forward and reverse path domain are both: {}",
                        args.forward_path.domain()
                    );

                    if self.state_internal.is_none() {
                        tracing::debug!("No previous `internal_state`. Copying...");
                        let mut ctx_internal = ctx.clone();

                        ctx_internal.generate_message_id().expect("bad state");
                        if let Ok(rcpt) = ctx_internal.forward_paths_mut() {
                            rcpt.clear();
                        }

                        self.state_internal = Some(
                            self.rule_engine.spawn_finished(
                                ctx_internal,
                                self.state
                                    .message()
                                    .read()
                                    .expect("message poisoned")
                                    .clone(),
                            ),
                        );
                    }

                    let internal_ctx = self
                        .state_internal
                        .as_ref()
                        .expect("has been set above")
                        .context();
                    let mut internal_guard = internal_ctx.write().expect("state poisoned");
                    internal_guard
                        .add_forward_path(
                            args.forward_path,
                            std::sync::Arc::new(Deliver::new(
                                self.rule_engine.srv().resolvers.get_resolver_root(),
                                self.config.clone(),
                            )),
                        )
                        .expect("bad state");
                    internal_guard
                        .set_transaction_type(TransactionType::Internal)
                        .expect("bad state");

                    ctx.set_transaction_type(TransactionType::Outgoing {
                        domain: reverse_path.expect("none-null reverse path").domain(),
                    })
                    .expect("bad state");

                    true
                }
                (true, _) => {
                    tracing::debug!(
                        "OUTGOING: reverse:${} => forward:${}",
                        reverse_path_domain.map_or("none".to_string(), |d| d.to_string()),
                        args.forward_path.domain()
                    );

                    ctx.add_forward_path(
                        args.forward_path,
                        std::sync::Arc::new(Deliver::new(
                            self.rule_engine.srv().resolvers.get_resolver_root(),
                            self.config.clone(),
                        )),
                    )
                    .expect("bad state");
                    ctx.set_transaction_type(reverse_path.as_ref().map_or(
                        TransactionType::Incoming(None),
                        |reverse_path| TransactionType::Outgoing {
                            domain: reverse_path.domain(),
                        },
                    ))
                    .expect("bad state");

                    false
                }
                (false, forward_path_is_handled) => {
                    tracing::debug!(
                        "INCOMING: reverse:${:?} => forward:${}",
                        reverse_path,
                        args.forward_path.domain()
                    );

                    ctx.set_transaction_type(TransactionType::Incoming(
                        if forward_path_is_handled {
                            Some(args.forward_path.domain())
                        } else {
                            None
                        },
                    ))
                    .expect("bad state");
                    ctx.add_forward_path(
                        args.forward_path,
                        std::sync::Arc::new(Deliver::new(
                            self.rule_engine.srv().resolvers.get_resolver_root(),
                            self.config.clone(),
                        )),
                    )
                    .expect("bad state");

                    false
                }
            }
        };

        let state = match self.state_internal.as_mut() {
            Some(state_internal) if is_internal => state_internal,
            _ => &mut self.state,
        };

        match self
            .rule_engine
            .run_when(state, &mut self.skipped, ExecutionStage::RcptTo)
        {
            Status::Faccept(reply) | Status::Accept(reply) | Status::Reject(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                "250 Ok\r\n".parse::<Reply>().unwrap()
            }
            Status::Deny(reply) => {
                ctx.deny();
                reply
            }
            Status::Delegated(_) => unreachable!(),
        }
    }

    async fn on_rset(&mut self) -> Reply {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .reset();

        self.state_internal = None;

        // TODO: reset message?

        "250 Ok\r\n".parse::<Reply>().unwrap()
    }

    async fn on_message(
        &mut self,
        ctx: &mut ReceiverContext,
        stream: impl tokio_stream::Stream<Item = Result<Vec<u8>, Error>> + Send + Unpin,
    ) -> (Reply, Option<Vec<Self::Item>>) {
        self.on_message_inner(ctx, stream).await
    }

    async fn on_message_completed(&mut self, item: Self::Item) -> Option<Reply> {
        let (ctx, msg) = item;
        self.on_message_completed_inner(ctx, msg).await
    }

    async fn on_hard_error(&mut self, ctx: &mut ReceiverContext, reply: Reply) -> Reply {
        ctx.deny();
        reply.extended(
            &"451 Too many errors from the client\r\n"
                .parse::<Reply>()
                .unwrap(),
        )
    }

    async fn on_soft_error(&mut self, _: &mut ReceiverContext, reply: Reply) -> Reply {
        tokio::time::sleep(self.config.server.smtp.error.delay).await;
        reply
    }

    fn get_stage(&self) -> Stage {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .stage()
    }
}
