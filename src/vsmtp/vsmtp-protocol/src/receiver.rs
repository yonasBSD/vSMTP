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
use crate::{
    reader::Reader, writer::WindowWriter, AcceptArgs, AuthArgs, ConnectionKind, EhloArgs, Error,
    HeloArgs, MailFromArgs, RcptToArgs, ReceiverHandler, Verb,
};
use tokio_rustls::rustls;
use tokio_stream::StreamExt;
use vsmtp_common::{auth::Mechanism, Reply, Stage};

enum HandshakeOutcome {
    Message,
    UpgradeTLS {
        config: alloc::sync::Arc<rustls::ServerConfig>,
        handshake_timeout: std::time::Duration,
    },
    Authenticate {
        mechanism: Mechanism,
        initial_response: Option<Vec<u8>>,
    },
    Quit,
}

pub struct ErrorCounter {
    pub error_count: i64,
    pub threshold_soft_error: i64,
    pub threshold_hard_error: i64,
}

/// An handle to send event from the [`ReceiverHandler`] to the [`Receiver`].
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct ReceiverContext {
    outcome: Option<HandshakeOutcome>,
}

impl ReceiverContext {
    /// Make the [`Receiver`] quit the connection early, and close cleanly.
    #[inline]
    pub fn deny(&mut self) {
        self.outcome = Some(HandshakeOutcome::Quit);
    }

    /// Make the [`Receiver`] initialize a TLS handshake.
    #[inline]
    pub fn upgrade_tls(
        &mut self,
        config: alloc::sync::Arc<rustls::ServerConfig>,
        handshake_timeout: std::time::Duration,
    ) {
        self.outcome = Some(HandshakeOutcome::UpgradeTLS {
            config,
            handshake_timeout,
        });
    }

    /// Make the [`Receiver`] initialize a SASL handshake.
    #[inline]
    pub fn authenticate(&mut self, mechanism: Mechanism, initial_response: Option<Vec<u8>>) {
        self.outcome = Some(HandshakeOutcome::Authenticate {
            mechanism,
            initial_response,
        });
    }
}

/// A SMTP receiver.
pub struct Receiver<
    H: ReceiverHandler + Send,
    V: rsasl::validate::Validation + Send,
    W: tokio::io::AsyncWrite + Unpin + Send,
    R: tokio::io::AsyncRead + Unpin + Send,
> where
    V::Value: Send + Sync,
{
    pub(crate) sink: WindowWriter<W>,
    pub(crate) stream: Reader<R>,
    error_counter: ErrorCounter,
    context: ReceiverContext,
    kind: ConnectionKind,
    message_size_max: usize,
    support_pipelining: bool,
    v: std::marker::PhantomData<V>,
    h: std::marker::PhantomData<H>,
}

impl<H: ReceiverHandler + Send, V: rsasl::validate::Validation + Send>
    Receiver<H, V, tokio::net::tcp::OwnedWriteHalf, tokio::net::tcp::OwnedReadHalf>
where
    V::Value: Send + Sync,
{
    fn upgrade_tls(
        self,
        handler: H,
        config: alloc::sync::Arc<rustls::ServerConfig>,
        handshake_timeout: std::time::Duration,
    ) -> impl tokio_stream::Stream<Item = Result<(), Error>> {
        async_stream::stream! {
            #[allow(clippy::expect_used)]
            let tcp_stream = self
                .sink
                .into_inner()
                .reunite(self.stream.into_inner())
                .expect("valid stream/sink pair");

            let acceptor = tokio_rustls::TlsAcceptor::from(config);

            let tls_tcp_stream = match tokio::time::timeout(
                handshake_timeout,
                acceptor.accept(tcp_stream),
            ).await {
                Ok(Ok(tls_tcp_stream)) => tls_tcp_stream,
                Ok(Err(e)) => {
                    Err(e)?;
                    return;
                }
                Err(_elapsed) => {
                    Err(Error::timeout(handshake_timeout, "tls handshake timed out"))?;
                    return;
                }
            };

            let tls_config = tls_tcp_stream.get_ref().1;
            let sni = tls_config.server_name().map(str::to_string);

            #[allow(clippy::expect_used)]
            let protocol_version = tls_config.protocol_version()
                .expect("tls handshake completed");
            #[allow(clippy::expect_used)]
            let negotiated_cipher_suite = tls_config.negotiated_cipher_suite()
                .expect("tls handshake completed");
            let peer_certificates = tls_config. peer_certificates()
                .map(<[rustls::Certificate]>::to_vec);
            let alpn_protocol = tls_config.alpn_protocol()
                .map(<[u8]>::to_vec);

            // FIXME: see https://github.com/tokio-rs/tls/issues/40
            let (read, write) = tokio::io::split(tls_tcp_stream);

            let (stream, sink) = (Reader::new(read, self.support_pipelining), WindowWriter::new(write));

            let secured_receiver = Receiver {
                sink,
                stream,
                context: ReceiverContext { outcome: None },
                error_counter: self.error_counter,
                kind: self.kind,
                message_size_max: self.message_size_max,
                support_pipelining: self.support_pipelining,
                v: self.v,
                h: self.h,
            }.into_secured_stream(
                handler,
                sni,
                protocol_version,
                negotiated_cipher_suite,
                peer_certificates,
                alpn_protocol
            );

            for await i in secured_receiver {
                yield i;
            }
        }
    }

    /// Create a new [`Receiver`] from a TCP/IP stream.
    #[inline]
    pub fn new(
        tcp_stream: tokio::net::TcpStream,
        kind: ConnectionKind,
        threshold_soft_error: i64,
        threshold_hard_error: i64,
        message_size_max: usize,
        support_pipelining: bool,
    ) -> Self {
        let (read, write) = tcp_stream.into_split();
        let (stream, sink) = (
            Reader::new(read, support_pipelining),
            WindowWriter::new(write),
        );
        Self {
            sink,
            stream,
            error_counter: ErrorCounter {
                error_count: 0,
                threshold_soft_error,
                threshold_hard_error,
            },
            context: ReceiverContext { outcome: None },
            kind,
            message_size_max,
            support_pipelining,
            v: std::marker::PhantomData,
            h: std::marker::PhantomData,
        }
    }
    /// Handle the inner stream to produce a [`tokio_stream::Stream`], each item
    /// being a successful SMTP transaction.
    ///
    /// # Panics
    ///
    /// * if the `on_accept` produces a `message` or a `authenticate` outcome (which is invalid)
    #[inline]
    pub fn into_stream<Fun, Future>(
        self,
        on_accept: Fun,
        client_addr: std::net::SocketAddr,
        server_addr: std::net::SocketAddr,
        timestamp: time::OffsetDateTime,
        uuid: uuid::Uuid,
    ) -> impl tokio_stream::Stream<Item = Result<(), ()>>
    where
        Fun: FnOnce(AcceptArgs) -> Future,
        Future: std::future::Future<Output = (H, ReceiverContext, Option<Reply>)>,
    {
        self.into_stream_with_error(on_accept, client_addr, server_addr, timestamp, uuid)
            .map(|e| match e {
                Ok(()) => Ok(()),
                Err(e) => {
                    tracing::error!(?e);
                    Err(())
                }
            })
    }

    #[allow(clippy::panic)]
    fn into_stream_with_error<Fun, Future>(
        mut self,
        on_accept: Fun,
        client_addr: std::net::SocketAddr,
        server_addr: std::net::SocketAddr,
        timestamp: time::OffsetDateTime,
        uuid: uuid::Uuid,
    ) -> impl tokio_stream::Stream<Item = Result<(), Error>>
    where
        Fun: FnOnce(AcceptArgs) -> Future,
        Future: std::future::Future<Output = (H, ReceiverContext, Option<Reply>)>,
    {
        async_stream::try_stream! {
            let accepted = on_accept(
                AcceptArgs {
                    client_addr,
                    server_addr,
                    kind: self.kind,
                    timestamp,
                    uuid,
                }
            ).await;
            let mut handler = match accepted {
                (mut handler, ReceiverContext{ outcome: None }, Some(reply_accept)) => {
                    self.sink
                        .direct_send_reply(&mut self.context, &mut self.error_counter, &mut handler, reply_accept)
                        .await?;
                    handler
                }
                (handler, ReceiverContext{
                    outcome: Some(HandshakeOutcome::UpgradeTLS {
                        config,
                        handshake_timeout
                    }),
                }, None) => {
                    for await i in self.upgrade_tls(handler, config, handshake_timeout) {
                        yield i?;
                    }
                    return;
                }
                (mut handler, ReceiverContext{ outcome: Some(HandshakeOutcome::Quit) }, reply_accept) => {
                    if let Some(reply_accept) = reply_accept {
                        self.sink
                            .direct_send_reply(&mut self.context, &mut self.error_counter, &mut handler, reply_accept)
                            .await?;
                    }
                    return;
                }
                _ => panic!("implementation of Handler is incorrect")
            };

            loop {
                match self.smtp_handshake(&mut handler).await? {
                    HandshakeOutcome::Message => {
                        let message_stream = self.stream.as_message_stream(self.message_size_max).fuse();
                        tokio::pin!(message_stream);

                        let (mut reply, completed) = handler.on_message(&mut self.context, message_stream).await;
                        if let Some(completed) = completed {
                            for item in completed {
                                if let Some(error) = handler.on_message_completed(item).await {
                                    reply = error;
                                    break;
                                }
                            }
                        }
                        self.sink
                            .direct_send_reply(&mut self.context, &mut self.error_counter, &mut handler, reply)
                            .await?;

                        yield ();
                    },
                    HandshakeOutcome::UpgradeTLS { config, handshake_timeout } => {
                        for await i in self.upgrade_tls(handler, config, handshake_timeout) {
                            yield i?;
                        }
                        return;
                    },
                    HandshakeOutcome::Authenticate { mechanism, initial_response } => {
                        let auth_result = self.authenticate(&mut handler, mechanism, initial_response).await;
                        // if security layer ...

                        let reply = handler.on_post_auth(&mut self.context, auth_result).await;
                        self.sink
                            .direct_send_reply(&mut self.context, &mut self.error_counter, &mut handler, reply)
                            .await?;

                        if matches!(std::mem::take(&mut self.context).outcome, Some(HandshakeOutcome::Quit)) {
                            return;
                        }

                    },
                    HandshakeOutcome::Quit => break,
                }
            }
        }
    }
}

impl<
        T: ReceiverHandler + Send,
        V: rsasl::validate::Validation + Send,
        W: tokio::io::AsyncWrite + Unpin + Send,
        R: tokio::io::AsyncRead + Unpin + Send,
    > Receiver<T, V, W, R>
where
    V::Value: Send + Sync,
{
    #[allow(clippy::panic)]
    fn into_secured_stream(
        mut self,
        mut handler: T,
        sni: Option<String>,
        protocol_version: rustls::ProtocolVersion,
        negotiated_cipher_suite: rustls::SupportedCipherSuite,
        peer_certificates: Option<Vec<rustls::Certificate>>,
        alpn_protocol: Option<Vec<u8>>,
    ) -> impl tokio_stream::Stream<Item = Result<(), Error>> {
        async_stream::try_stream! {
            let reply_post_tls_handshake = handler.on_post_tls_handshake(
                sni,
                protocol_version,
                negotiated_cipher_suite.suite(),
                peer_certificates,
                alpn_protocol
            ).await;

            if self.kind == ConnectionKind::Tunneled {
                self.sink.direct_send_reply(
                    &mut self.context,
                    &mut self.error_counter,
                    &mut handler,
                    reply_post_tls_handshake
                ).await?;
            }

            loop {
                match self.smtp_handshake(&mut handler).await? {
                    HandshakeOutcome::Message => {
                        let message_stream = self.stream.as_message_stream(self.message_size_max).fuse();
                        tokio::pin!(message_stream);

                        let (mut reply, completed) = handler.on_message(&mut self.context, message_stream).await;
                        if let Some(completed) = completed {
                            for item in completed {
                                if let Some(error) = handler.on_message_completed(item).await {
                                    reply = error;
                                    break;
                                }
                            }
                        }
                        self.sink
                            .direct_send_reply(&mut self.context, &mut self.error_counter, &mut handler, reply)
                            .await?;

                        yield ();
                    },
                    HandshakeOutcome::UpgradeTLS { .. } => panic!("smtp_handshake should not return UpgradeTLS"),
                    HandshakeOutcome::Authenticate { mechanism, initial_response } => {
                        let auth_result = self.authenticate(&mut handler, mechanism, initial_response).await;
                        // if security layer ...

                        let reply = handler.on_post_auth(&mut self.context, auth_result).await;
                        self.sink
                            .direct_send_reply(&mut self.context, &mut self.error_counter, &mut handler, reply)
                            .await?;

                        if matches!(std::mem::take(&mut self.context).outcome, Some(HandshakeOutcome::Quit)) {
                            return;
                        }

                    },
                    HandshakeOutcome::Quit => break,
                }
            }
        }
    }

    /// SMTP handshake (generate the envelope and metadata).
    ///
    /// # Returns
    ///
    /// * the `Vec<u8>` is the bytes read with the SMTP verb "DATA\r\n"
    #[allow(clippy::too_many_lines)]
    async fn smtp_handshake(&mut self, handler: &mut T) -> Result<HandshakeOutcome, Error> {
        macro_rules! handle_args {
            ($args_output:ty, $args:expr, $on_event:tt) => {
                match <$args_output>::try_from($args) {
                    Ok(args) => handler.$on_event(&mut self.context, args).await,
                    Err(e) => handler.on_args_error(&e).await,
                }
            };
            ($args_output:ty, $args:expr, Option: $on_event:tt) => {
                match <$args_output>::try_from($args) {
                    Ok(args) => handler.$on_event(&mut self.context, args).await,
                    Err(e) => Some(handler.on_args_error(&e).await),
                }
            };
        }

        let command_stream = self
            .stream
            .as_window_stream()
            .timeout(std::time::Duration::from_secs(30));
        tokio::pin!(command_stream);

        loop {
            let commands_batch = match command_stream.try_next().await {
                // FIXME: remove intermediate result
                Ok(Some(Ok(commands_batch))) if !commands_batch.is_empty() => commands_batch,
                Err(e) => {
                    tracing::warn!("Closing after {} without receiving a command", e);
                    #[allow(clippy::expect_used)]
                    self.sink
                        .direct_send_reply(
                            &mut self.context,
                            &mut self.error_counter,
                            handler,
                            "451 Timeout - closing connection\r\n"
                                .parse()
                                .expect("valid syntax"),
                        )
                        .await?;

                    return Ok(HandshakeOutcome::Quit);
                }
                _ => return Ok(HandshakeOutcome::Quit),
            };
            for command in commands_batch {
                let (verb, args) = match command {
                    Ok(command) => command,
                    Err(e) => {
                        if let Some(e) = e.get_ref().and_then(
                            <(dyn std::error::Error
                                 + std::marker::Send
                                 + std::marker::Sync
                                 + 'static)>::downcast_ref,
                        ) {
                            let reply = handler.on_args_error(e).await;
                            self.sink
                                .direct_send_reply(
                                    &mut self.context,
                                    &mut self.error_counter,
                                    handler,
                                    reply,
                                )
                                .await?;
                            continue;
                        }
                        tracing::error!(?e);
                        return Err(e);
                    }
                };
                tracing::trace!("<< {:?} ; {:?}", verb, std::str::from_utf8(&args.0));

                let stage = handler.get_stage();
                let reply = match (verb, stage) {
                    (Verb::Helo, _) => Some(handle_args!(HeloArgs, args, on_helo)),
                    (Verb::Ehlo, _) => Some(handle_args!(EhloArgs, args, on_ehlo)),
                    (Verb::Noop, _) => Some(handler.on_noop().await),
                    (Verb::Rset, _) => Some(handler.on_rset().await),
                    (Verb::StartTls, Stage::Connect | Stage::Helo) => {
                        Some(handler.on_starttls(&mut self.context).await)
                    }
                    (Verb::Auth, Stage::Connect | Stage::Helo) => {
                        handle_args!(AuthArgs, args, Option: on_auth)
                    }
                    (Verb::MailFrom, Stage::Helo | Stage::MailFrom) => {
                        Some(handle_args!(MailFromArgs, args, on_mail_from))
                    }
                    (Verb::RcptTo, Stage::MailFrom | Stage::RcptTo) => {
                        Some(handle_args!(RcptToArgs, args, on_rcpt_to))
                    }
                    (Verb::Data, Stage::RcptTo) => {
                        self.context.outcome = Some(HandshakeOutcome::Message);
                        Some(handler.on_data().await)
                    }
                    (Verb::Quit, _) => {
                        self.context.outcome = Some(HandshakeOutcome::Quit);
                        Some(handler.on_quit().await)
                    }
                    (Verb::Help, _) => Some(handler.on_help(args).await),
                    (Verb::Unknown, _) => Some(handler.on_unknown(args.0).await),
                    otherwise => Some(handler.on_bad_sequence(otherwise).await),
                };
                if let Some(reply) = reply {
                    self.sink
                        .send_reply(
                            &mut self.context,
                            &mut self.error_counter,
                            handler,
                            reply,
                            verb,
                        )
                        .await?;
                }
            }

            if !self.sink.is_empty() {
                self.sink.flush().await?;
            }
            if let Some(done) = std::mem::take(&mut self.context).outcome {
                return Ok(done);
            }
        }
    }
}
