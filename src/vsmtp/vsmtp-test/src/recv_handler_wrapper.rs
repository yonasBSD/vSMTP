/*
 * vSMTP mail transfer agent
 * Copyright (C) 2023 viridIT SAS
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

extern crate alloc;

use tokio_rustls::rustls;
use vsmtp_common::ContextFinished;
use vsmtp_common::{Reply, Stage};
use vsmtp_config::Config;
use vsmtp_mail_parser::MessageBody;
use vsmtp_protocol::{
    AcceptArgs, AuthArgs, AuthError, CallbackWrap, EhloArgs, Error, HeloArgs, MailFromArgs,
    RcptToArgs, ReceiverContext, ReceiverHandler,
};

// NOTE: could be enhance to allow entry point on each call
///
pub trait OnMessageCompletedHook {
    fn on_message_completed(self, ctx: ContextFinished, msg: MessageBody);
}

impl<F> OnMessageCompletedHook for F
where
    F: FnOnce(ContextFinished, MessageBody),
{
    fn on_message_completed(self, ctx: ContextFinished, msg: MessageBody) {
        (self)(ctx, msg);
    }
}

/// wrapper around `ReceiverHandler` which simplify implementation of new handler
pub struct Wrapper<Inner: ReceiverHandler, Hook: OnMessageCompletedHook> {
    ///
    pub inner: Inner,
    ///
    pub hook: Hook,
}

#[async_trait::async_trait]
impl<Inner, Hook> ReceiverHandler for Wrapper<Inner, Hook>
where
    Inner: ReceiverHandler + Send,
    Hook: OnMessageCompletedHook + Clone + Send,
{
    fn get_stage(&self) -> Stage {
        self.inner.get_stage()
    }

    fn get_config(&self) -> alloc::sync::Arc<Config> {
        self.inner.get_config()
    }

    fn generate_sasl_callback(&self) -> CallbackWrap {
        self.inner.generate_sasl_callback()
    }

    async fn on_accept(&mut self, ctx: &mut ReceiverContext, args: AcceptArgs) -> Reply {
        self.inner.on_accept(ctx, args).await
    }

    async fn on_starttls(&mut self, ctx: &mut ReceiverContext) -> Reply {
        self.inner.on_starttls(ctx).await
    }

    async fn on_post_tls_handshake(
        &mut self,
        sni: Option<String>,
        protocol_version: rustls::ProtocolVersion,
        cipher_suite: rustls::CipherSuite,
        peer_certificates: Option<Vec<rustls::Certificate>>,
        alpn_protocol: Option<Vec<u8>>,
    ) -> Reply {
        self.inner
            .on_post_tls_handshake(
                sni,
                protocol_version,
                cipher_suite,
                peer_certificates,
                alpn_protocol,
            )
            .await
    }

    async fn on_auth(&mut self, ctx: &mut ReceiverContext, args: AuthArgs) -> Option<Reply> {
        self.inner.on_auth(ctx, args).await
    }

    async fn on_post_auth(
        &mut self,
        ctx: &mut ReceiverContext,
        result: Result<(), AuthError>,
    ) -> Reply {
        self.inner.on_post_auth(ctx, result).await
    }

    async fn on_helo(&mut self, ctx: &mut ReceiverContext, args: HeloArgs) -> Reply {
        self.inner.on_helo(ctx, args).await
    }

    async fn on_ehlo(&mut self, ctx: &mut ReceiverContext, args: EhloArgs) -> Reply {
        self.inner.on_ehlo(ctx, args).await
    }

    async fn on_mail_from(&mut self, ctx: &mut ReceiverContext, args: MailFromArgs) -> Reply {
        self.inner.on_mail_from(ctx, args).await
    }

    async fn on_rcpt_to(&mut self, ctx: &mut ReceiverContext, args: RcptToArgs) -> Reply {
        self.inner.on_rcpt_to(ctx, args).await
    }

    async fn on_message(
        &mut self,
        ctx: &mut ReceiverContext,
        stream: impl tokio_stream::Stream<Item = Result<Vec<u8>, Error>> + Send + Unpin,
    ) -> (Reply, Option<Vec<(ContextFinished, MessageBody)>>) {
        self.inner.on_message(ctx, stream).await
    }

    async fn on_message_completed(
        &mut self,
        ctx: ContextFinished,
        msg: MessageBody,
    ) -> Option<Reply> {
        // self.inner.on_message_completed(ctx, msg).await
        self.hook.clone().on_message_completed(ctx, msg);
        None
    }

    async fn on_hard_error(&mut self, ctx: &mut ReceiverContext, reply: Reply) -> Reply {
        self.inner.on_hard_error(ctx, reply).await
    }

    async fn on_soft_error(&mut self, ctx: &mut ReceiverContext, reply: Reply) -> Reply {
        self.inner.on_soft_error(ctx, reply).await
    }

    async fn on_rset(&mut self) -> Reply {
        self.inner.on_rset().await
    }
}
