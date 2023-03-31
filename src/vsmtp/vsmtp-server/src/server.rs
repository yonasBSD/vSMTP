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
use crate::{receiver::handler::Handler, scheduler::Emitter, ValidationVSL};
use anyhow::Context;
use tokio_rustls::rustls;
use tokio_stream::StreamExt;
use vqueue::GenericQueueManager;
use vsmtp_common::Reply;
use vsmtp_config::{get_rustls_config, Config};
use vsmtp_mail_parser::BasicParser;
use vsmtp_protocol::{AcceptArgs, ConnectionKind};
use vsmtp_rule_engine::RuleEngine;

/// TCP/IP server
pub struct Server {
    conn_max_reach_reply: Reply,

    config: std::sync::Arc<Config>,
    tls_config: Option<std::sync::Arc<rustls::ServerConfig>>,
    rule_engine: std::sync::Arc<RuleEngine>,
    queue_manager: std::sync::Arc<dyn GenericQueueManager>,
    emitter: std::sync::Arc<Emitter>,
}

/// Create a `TCPListener` ready to be listened to
///
/// # Errors
///
/// * failed to bind to the socket address
/// * failed to set the listener to non blocking
pub fn socket_bind_anyhow<A: std::net::ToSocketAddrs + std::fmt::Debug>(
    addr: A,
) -> anyhow::Result<std::net::TcpListener> {
    let socket = std::net::TcpListener::bind(&addr)
        .with_context(|| format!("Failed to bind socket on addr: '{addr:?}'"))?;

    socket
        .set_nonblocking(true)
        .with_context(|| format!("Failed to set non-blocking socket on addr: '{addr:?}'"))?;

    Ok(socket)
}

type ListenerStreamItem = std::io::Result<(tokio::net::TcpStream, std::net::SocketAddr)>;

fn listener_to_stream(
    listener: &tokio::net::TcpListener,
) -> impl tokio_stream::Stream<Item = ListenerStreamItem> + '_ {
    async_stream::try_stream! {
        loop {
            yield listener.accept().await?;
        }
    }
}

impl Server {
    /// Create a server with the configuration provided, and the sockets already bound
    ///
    /// # Errors
    ///
    /// * `spool_dir` does not exist and failed to be created
    /// * cannot convert sockets to `[tokio::net::TcpListener]`
    /// * cannot initialize [rustls] config
    pub fn new(
        config: std::sync::Arc<Config>,
        rule_engine: std::sync::Arc<RuleEngine>,
        queue_manager: std::sync::Arc<dyn GenericQueueManager>,
        emitter: std::sync::Arc<Emitter>,
    ) -> anyhow::Result<Self> {
        if !config.server.queues.dirpath.exists() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .create(&config.server.queues.dirpath)?;
        }

        Ok(Self {
            conn_max_reach_reply: "554 Cannot process connection, closing\r\n"
                .parse::<Reply>()
                .expect("valid smtp reply"),
            tls_config: if let Some(smtps) = &config.server.tls {
                Some(std::sync::Arc::new(get_rustls_config(
                    smtps,
                    &config.server.r#virtual,
                )?))
            } else {
                None
            },
            rule_engine,
            queue_manager,
            config,
            emitter,
        })
    }

    #[tracing::instrument(name = "handle-client", skip_all, fields(client = %client_addr, server = %server_addr))]
    async fn handle_client(
        &self,
        client_counter: std::sync::Arc<std::sync::atomic::AtomicI64>,
        kind: ConnectionKind,
        mut stream: tokio::net::TcpStream,
        client_addr: std::net::SocketAddr,
        server_addr: std::net::SocketAddr,
    ) {
        tracing::info!(%kind, "Connection accepted.");

        if self.config.server.client_count_max != -1
            && client_counter.load(std::sync::atomic::Ordering::SeqCst)
                >= self.config.server.client_count_max
        {
            tracing::warn!(
                max = self.config.server.client_count_max,
                "Connection count max reached, rejecting connection.",
            );

            if let Err(error) = tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                self.conn_max_reach_reply.as_ref().as_bytes(),
            )
            .await
            {
                tracing::error!(%error, "Code delivery failure.");
            }

            if let Err(error) = tokio::io::AsyncWriteExt::shutdown(&mut stream).await {
                tracing::error!(%error, "Closing connection failure.");
            }
            return;
        }

        client_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let session = Self::serve(
            AcceptArgs::new(
                client_addr,
                stream.local_addr().expect("retrieve local address"),
                time::OffsetDateTime::now_utc(),
                uuid::Uuid::new_v4(),
                kind,
            ),
            stream,
            self.tls_config.clone(),
            self.config.clone(),
            self.rule_engine.clone(),
            self.queue_manager.clone(),
            self.emitter.clone(),
        );
        let client_counter_copy = client_counter.clone();
        tokio::spawn(async move {
            let _err = session.await;

            client_counter_copy.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }

    /// Main loop of `vSMTP`'s server
    ///
    /// # Errors
    ///
    /// * failed to convert sockets to `[tokio::net::TcpListener]`
    #[tracing::instrument(skip_all)]
    pub async fn listen(
        self,
        sockets: (
            Vec<std::net::TcpListener>,
            Vec<std::net::TcpListener>,
            Vec<std::net::TcpListener>,
        ),
    ) -> anyhow::Result<()> {
        fn to_tokio(
            s: Vec<std::net::TcpListener>,
        ) -> std::io::Result<Vec<tokio::net::TcpListener>> {
            s.into_iter()
                .map(tokio::net::TcpListener::from_std)
                .collect::<std::io::Result<Vec<tokio::net::TcpListener>>>()
        }

        if self.config.server.tls.is_none() && !sockets.2.is_empty() {
            tracing::warn!(
                "No TLS configuration provided, listening on submissions protocol (port 465) will cause issue"
            );
        }

        let client_counter = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));

        let (listener, listener_submission, listener_tunneled) = (
            to_tokio(sockets.0)?,
            to_tokio(sockets.1)?,
            to_tokio(sockets.2)?,
        );

        let mut map = tokio_stream::StreamMap::new();
        for (kind, sockets) in [
            (ConnectionKind::Relay, &listener),
            (ConnectionKind::Submission, &listener_submission),
            (ConnectionKind::Tunneled, &listener_tunneled),
        ] {
            for listener in sockets {
                let accept = listener_to_stream(listener);
                let transform = tokio_stream::StreamExt::map(accept, move |client| (kind, client));

                map.insert(
                    listener.local_addr().expect("retrieve local address"),
                    Box::pin(transform),
                );
            }
        }

        tracing::info!(
            interfaces = ?map.keys().collect::<Vec<_>>(),
            "Listening for clients.",
        );

        while let Some((server_addr, (kind, client))) =
            tokio_stream::StreamExt::next(&mut map).await
        {
            let (stream, client_addr) = client?;

            self.handle_client(
                client_counter.clone(),
                kind,
                stream,
                client_addr,
                server_addr,
            )
            .await;
        }
        Ok(())
    }

    ///
    /// # Errors
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all, err, fields(uuid = %args.uuid))]
    pub async fn serve(
        args: AcceptArgs,
        tcp_stream: tokio::net::TcpStream,
        tls_config: Option<std::sync::Arc<rustls::ServerConfig>>,
        config: std::sync::Arc<Config>,
        rule_engine: std::sync::Arc<RuleEngine>,
        queue_manager: std::sync::Arc<dyn GenericQueueManager>,
        emitter: std::sync::Arc<Emitter>,
    ) -> anyhow::Result<()> {
        let smtp_handler = Handler::new(
            config.clone(),
            tls_config,
            rule_engine,
            queue_manager,
            BasicParser::default,
            emitter,
            args.client_addr,
            args.server_addr,
            config.server.name.clone(),
            args.timestamp,
            args.uuid,
        );
        let smtp_receiver = vsmtp_protocol::Receiver::<_, ValidationVSL, _, _>::new(
            tcp_stream,
            args.kind,
            smtp_handler,
            config.server.smtp.error.soft_count,
            config.server.smtp.error.hard_count,
            config.server.message_size_limit,
        );
        let smtp_stream = smtp_receiver.into_stream(
            args.client_addr,
            args.server_addr,
            args.timestamp,
            args.uuid,
        );
        tokio::pin!(smtp_stream);

        while matches!(smtp_stream.next().await, Some(Ok(()))) {}

        log::info!("Connection closed cleanly.");
        Ok(())
    }
}
