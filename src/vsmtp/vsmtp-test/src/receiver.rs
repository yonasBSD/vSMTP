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

/// run a connection and assert output produced by `vSMTP` and `expected_output`
#[macro_export]
macro_rules! run_test {
    (
        input = $input:expr,
        expected = $expected:expr
        $(, starttls $( = $server_name_starttls:expr )? => $secured_input:expr)?
        $(, tunnel = $server_name_tunnel:expr)?
        $(, config = $config:expr)?
        $(, config_arc = $config_arc:expr)?
        $(, mail_handler = $mail_handler:expr)?
        $(, hierarchy_builder = $hierarchy_builder:expr)?
        $(,)?
    ) => {{
        use tokio_rustls::rustls;
        #[allow(dead_code)]
        async fn upgrade_tls(server_name: &str, stream: tokio::net::TcpStream) -> tokio_rustls::client::TlsStream<tokio::net::TcpStream> {
            struct CertVerifier {
                webpki: rustls::client::WebPkiVerifier,
            }

            impl rustls::client::ServerCertVerifier for CertVerifier {
                fn verify_server_cert(
                    &self,
                    end_entity: &rustls::Certificate,
                    intermediates: &[rustls::Certificate],
                    server_name: &rustls::ServerName,
                    scts: &mut dyn Iterator<Item = &[u8]>,
                    ocsp_response: &[u8],
                    now: std::time::SystemTime
                ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
                    match self.webpki.verify_server_cert(
                        end_entity,
                        intermediates,
                        server_name,
                        scts,
                        ocsp_response,
                        now
                    ) {
                        Ok(res) => Ok(res),
                        // got this error when not using SNI
                        Err(
                            rustls::Error::InvalidCertificate(
                                rustls::CertificateError::NotValidForName
                            ) | rustls::Error::UnsupportedNameType) => Ok(
                                rustls::client::ServerCertVerified::assertion()
                            ),
                        Err(e) => Err(e)
                    }
                }
            }

            const TEST_SERVER_CERT: &str = "src/template/certs/certificate.crt";
            const TEST_SERVER_KEY: &str = "src/template/certs/private_key.rsa.key";

            let mut reader = std::io::BufReader::new(std::fs::File::open(TEST_SERVER_CERT).unwrap());

            let pem = rustls_pemfile::certs(&mut reader)
                .unwrap()
                .into_iter()
                .map(rustls::Certificate)
                .collect::<Vec<_>>();

            let mut root_store = rustls::RootCertStore::empty();
            for i in pem {
                root_store.add(&i).unwrap();
            }

            let client_config = std::sync::Arc::new(rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_custom_certificate_verifier(std::sync::Arc::new(
                    CertVerifier {
                        webpki: rustls::client::WebPkiVerifier::new(root_store, None),
                    }
                ))
                .with_no_client_auth()
            );

            let connector = tokio_rustls::TlsConnector::from(client_config.clone());
            connector
                .connect(
                    if server_name == "127.0.0.1" {
                        rustls::ServerName::IpAddress("127.0.0.1".parse().unwrap())
                    } else {
                        rustls::ServerName::try_from(server_name).unwrap()
                    },
                    stream
                ).await.unwrap()
        }

        let expected: Vec<String> = $expected.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let input: Vec<String> = $input.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();

        $( let secured_input: Vec<String> = $secured_input.into_iter().map(|s| s.to_string()).collect::<Vec<_>>(); )?

        $( let server_name: &str = $server_name_tunnel; )?
        $( let server_name: &str = {
            #[allow(clippy::no_effect)]
            $secured_input;
            let _name = "127.0.0.1";
            $( let _name = $server_name_starttls; )?
            _name
        }; )?

        let (socket_server, server_addr) = loop {
            let port = rand::random::<u32>().rem_euclid(65535 - 1025) + 1025;
            let server_addr: std::net::SocketAddr = format!("0.0.0.0:{port}").parse().expect("valid address");
            match tokio::net::TcpListener::bind(server_addr.clone()).await {
                Ok(socket_server) => break (socket_server, server_addr),
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => (),
                Err(e) => panic!("{}", e),
            };
        };

        let config: std::sync::Arc<vsmtp_config::Config> =  {
            let _f = || std::sync::Arc::new($crate::config::local_test());      $(
            let _f = || std::sync::Arc::new($config);                       )?  $(
            let _f = || $config_arc;                                        )?
            _f()
        };

        let queue_manager =
            <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![]).unwrap();

        let queue_manager_cloned = std::sync::Arc::clone(&queue_manager);

        let server = tokio::spawn(async move {
            let kind = {
                let _f = || vsmtp_protocol::ConnectionKind::Relay;                  $(
                let _f = || {
                    #[allow(clippy::no_effect)]
                    $server_name_tunnel;
                    vsmtp_protocol::ConnectionKind::Tunneled
                };)?
                _f()
            };

            let resolvers = std::sync::Arc::new(vsmtp_config::DnsResolvers::from_config(&config).unwrap());

            let (emitter, _working_rx, _delivery_rx) = vsmtp_server::scheduler::init(1, 1);

            let rule_engine: std::sync::Arc<vsmtp_rule_engine::RuleEngine> = {
                let _f = || vsmtp_rule_engine::RuleEngine::new(
                    config.clone(),
                    resolvers.clone(),
                    queue_manager.clone()
                ).unwrap();                                         $(
                let _f = || vsmtp_rule_engine::RuleEngine::with_hierarchy(
                    $hierarchy_builder,
                    config.clone(),
                    resolvers.clone(),
                    queue_manager.clone()
                ).unwrap();                                         )?
                std::sync::Arc::new(_f())
            };
            let (client_stream, client_addr) = socket_server.accept().await.unwrap();

            let smtp_receiver = vsmtp_protocol::Receiver::<_, vsmtp_server::ValidationVSL, _, _>::new(
                client_stream,
                kind,
                config.server.smtp.error.soft_count,
                config.server.smtp.error.hard_count,
                config.server.message_size_limit,
                config.server.esmtp.pipelining,
            );
            let smtp_stream = smtp_receiver.into_stream(
                |args| async move {
                    let smtp_handler = || vsmtp_server::Handler::on_accept(
                        args,
                        rule_engine,
                        config.clone(),
                        {
                            let _tls_config = Option::<std::sync::Arc<rustls::ServerConfig>>::None;
                            $( #[allow(clippy::no_effect)] $server_name_tunnel;
                            let _tls_config = config.server.tls.as_ref().map(|tls| {
                                arc!(vsmtp_config::get_rustls_config(
                                    tls, &config.server.r#virtual,
                                ).unwrap())
                            }); )?

                            $( #[allow(clippy::no_effect)] $secured_input;
                            let _tls_config = config.server.tls.as_ref().map(|tls| {
                                arc!(vsmtp_config::get_rustls_config(
                                    tls, &config.server.r#virtual,
                                ).unwrap())
                            }); )?

                            _tls_config
                        },
                        queue_manager,
                        emitter,
                        vsmtp_mail_parser::BasicParser::default,
                    );

                    let _f = smtp_handler;          $(
                    let _f = || {
                        let (inner, ctx, reply) = _f();
                        ($crate::Wrapper{
                            inner,
                            hook: $mail_handler,
                        }, ctx, reply)
                        }; )?
                    _f()
                },
                client_addr,
                server_addr,
                time::OffsetDateTime::now_utc(),
                uuid::Uuid::new_v4()
            );
            tokio::pin!(smtp_stream);

            while matches!(tokio_stream::StreamExt::next(&mut smtp_stream).await, Some(Ok(()))) {}
        });

        let client = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            use tokio::io::AsyncWriteExt;
            let stream = tokio::net::TcpStream::connect(server_addr)
                .await
                .unwrap();

            $( let stream = {
                #[allow(clippy::no_effect)] $server_name_tunnel;
                upgrade_tls(server_name, stream).await
            }; )?
            let mut stream = tokio::io::BufReader::new(stream);

            let mut output = vec![];
            let mut line_to_send = input.iter().cloned();

            loop {
                let mut line_received = String::new();
                // read until '\n' or '\r\n'
                if stream.read_line(&mut line_received).await.map_or(true, |l| l == 0) {
                    break;
                }

                output.push(line_received);
                if output.last().unwrap().chars().nth(3) == Some('-') { continue; }
                match line_to_send.next() {
                    Some(line) => stream.write_all(line.as_bytes()).await.unwrap(),
                    None => break,
                }
            }
            $(
                #[allow(clippy::no_effect)] $secured_input;

                if !output.last().unwrap().starts_with("220 ") {
                    todo!();
                }

                let stream = upgrade_tls(server_name, stream.into_inner()).await;
                let mut stream = tokio::io::BufReader::new(stream);

                let mut line_to_send = secured_input.iter().cloned();

                stream.write_all(line_to_send.next().unwrap().as_bytes()).await.unwrap();

                loop {
                    let mut line_received = String::new();
                    // read until '\n' or '\r\n'
                    if stream.read_line(&mut line_received).await.map_or(true, |l| l == 0) {
                        break;
                    }

                    output.push(line_received);
                    if output.last().unwrap().chars().nth(3) == Some('-') { continue; }
                    match line_to_send.next() {
                        Some(line) => stream.write_all(line.as_bytes()).await.unwrap(),
                        None => break,
                    }
                }
            )?

            output
        });

        let (client, server) = tokio::join!(client, server);
        let (client, _server) = (client.unwrap(), server.unwrap());

        pretty_assertions::assert_eq!(expected, client);

        queue_manager_cloned
    }};
    (
        fn $name:ident,
        input = $input:expr,
        expected = $expected:expr
        $(, starttls $( = $server_name_starttls:expr )? => $secured_input:expr)?
        $(, tunnel = $server_name_tunnel:expr)?
        $(, config = $config:expr)?
        $(, config_arc = $config_arc:expr)?
        $(, mail_handler = $mail_handler:expr)?
        $(, hierarchy_builder = $hierarchy_builder:expr)?
        $(,)?
    ) => {
        #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
        async fn $name() {
            run_test! {
                input = $input,
                expected = $expected
                $(, starttls $( = $server_name_starttls )? => $secured_input)?
                $(, tunnel = $server_name_tunnel)?
                $(, config = $config)?
                $(, config_arc = $config_arc)?
                $(, mail_handler = $mail_handler)?
                $(, hierarchy_builder = $hierarchy_builder)?
            };
        }
    };
}

/// macro used to test pipelined test
/// It reads full content of TCP buffer to make expected assert.
/// TODO: some cleaning should be necessary in parameters. I did not understand the code enough to do it.
#[macro_export]
macro_rules! run_pipelined_test {
    (
        input = $input:expr,
        expected = $expected:expr
        $(, starttls $( = $server_name_starttls:expr )? => $secured_input:expr)?
        $(, tunnel = $server_name_tunnel:expr)?
        $(, config = $config:expr)?
        $(, config_arc = $config_arc:expr)?
        $(, mail_handler = $mail_handler:expr)?
        $(, hierarchy_builder = $hierarchy_builder:expr)?
        $(,)?
    ) => {{
        use tokio_rustls::rustls;
        let expected: Vec<String> = $expected.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let input: Vec<String> = $input.into_iter().map(|s| s.to_string()).collect::<Vec<_>>();

        $( let secured_input: Vec<String> = $secured_input.into_iter().map(|s| s.to_string()).collect::<Vec<_>>(); )?

        $( let server_name: &str = $server_name_tunnel; )?
        $( let server_name: &str = {
            #[allow(clippy::no_effect)]
            $secured_input;
            let _name = "127.0.0.1";
            $( let _name = $server_name_starttls; )?
            _name
        }; )?

        let (socket_server, server_addr) = loop {
            let port = rand::random::<u32>().rem_euclid(65535 - 1025) + 1025;
            let server_addr: std::net::SocketAddr = format!("0.0.0.0:{port}").parse().expect("valid address");
            match tokio::net::TcpListener::bind(server_addr.clone()).await {
                Ok(socket_server) => break (socket_server, server_addr),
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => (),
                Err(e) => panic!("{}", e),
            };
        };

        let config: std::sync::Arc<vsmtp_config::Config> =  {
            let _f = || std::sync::Arc::new($crate::config::local_test());      $(
            let _f = || std::sync::Arc::new($config);                       )?  $(
            let _f = || $config_arc;                                        )?
            _f()
        };

        let queue_manager =
            <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![]).unwrap();

        let queue_manager_cloned = std::sync::Arc::clone(&queue_manager);

        let server = tokio::spawn(async move {
            let kind = {
                let _f = || vsmtp_protocol::ConnectionKind::Relay;                  $(
                let _f = || {
                    #[allow(clippy::no_effect)]
                    $server_name_tunnel;
                    vsmtp_protocol::ConnectionKind::Tunneled
                };)?
                _f()
            };

            let resolvers = std::sync::Arc::new(vsmtp_config::DnsResolvers::from_config(&config).unwrap());

            let (emitter, _working_rx, _delivery_rx) = vsmtp_server::scheduler::init(1, 1);

            let rule_engine: std::sync::Arc<vsmtp_rule_engine::RuleEngine> = {
                let _f = || vsmtp_rule_engine::RuleEngine::new(
                    config.clone(),
                    resolvers.clone(),
                    queue_manager.clone()
                ).unwrap();                                         $(
                let _f = || vsmtp_rule_engine::RuleEngine::with_hierarchy(
                    $hierarchy_builder,
                    config.clone(),
                    resolvers.clone(),
                    queue_manager.clone()
                ).unwrap();                                         )?
                std::sync::Arc::new(_f())
            };
            let (client_stream, client_addr) = socket_server.accept().await.unwrap();

            let smtp_receiver = vsmtp_protocol::Receiver::<_, vsmtp_server::ValidationVSL, _, _>::new(
                client_stream,
                kind,
                config.server.smtp.error.soft_count,
                config.server.smtp.error.hard_count,
                config.server.message_size_limit,
                config.server.esmtp.pipelining,
            );
            let smtp_stream = smtp_receiver.into_stream(
                |args| async move {
                    let smtp_handler = || vsmtp_server::Handler::on_accept(
                        args,
                        rule_engine,
                        config.clone(),
                        {
                            let _tls_config = Option::<std::sync::Arc<rustls::ServerConfig>>::None;
                            $( #[allow(clippy::no_effect)] $server_name_tunnel;
                            let _tls_config = config.server.tls.as_ref().map(|tls| {
                                arc!(vsmtp_config::get_rustls_config(
                                    tls, &config.server.r#virtual,
                                ).unwrap())
                            }); )?

                            $( #[allow(clippy::no_effect)] $secured_input;
                            let _tls_config = config.server.tls.as_ref().map(|tls| {
                                arc!(vsmtp_config::get_rustls_config(
                                    tls, &config.server.r#virtual,
                                ).unwrap())
                            }); )?

                            _tls_config
                        },
                        queue_manager,
                        emitter,
                        vsmtp_mail_parser::BasicParser::default,
                    );

                    let _f = smtp_handler;          $(
                    let _f = || {
                        let (inner, ctx, reply) = _f();
                        ($crate::Wrapper{
                            inner,
                            hook: $mail_handler,
                        }, ctx, reply)
                        }; )?
                    _f()
                },
                client_addr,
                server_addr,
                time::OffsetDateTime::now_utc(),
                uuid::Uuid::new_v4()
            );
            tokio::pin!(smtp_stream);

            while matches!(tokio_stream::StreamExt::next(&mut smtp_stream).await, Some(Ok(()))) {}
        });

        let client = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            use tokio::io::AsyncWriteExt;
            let stream = tokio::net::TcpStream::connect(server_addr)
                .await
                .unwrap();

            $( let stream = {
                #[allow(clippy::no_effect)] $server_name_tunnel;
            }; )?
            let mut stream = tokio::io::BufReader::new(stream);

            let mut output = vec![];
            let mut line_to_send = input.iter().cloned();

            loop {
                let mut line_received = String::new();
                let read_timeout = tokio::time::Duration::from_millis(50);
                let mut tmp_line_received = String::new();
                loop {
                    match tokio::time::timeout(read_timeout, stream.read_line(&mut tmp_line_received)).await {
                        Ok(_) => {
                            // at the end of the buffer, no error is raised but tmp_line_received is not changed
                            if line_received == tmp_line_received {
                                break;
                            }
                            line_received = tmp_line_received.clone();
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
                if line_received.len() == 0 {
                    break;
                }
                output.push(line_received);
                match line_to_send.next() {
                    Some(line) => stream.write_all(line.as_bytes()).await.unwrap(),
                    None => break,
                }
            }
            $(
                #[allow(clippy::no_effect)] $secured_input;

                if !output.last().unwrap().starts_with("220 ") {
                    todo!();
                }

                let stream = upgrade_tls(server_name, stream.into_inner()).await;
                let mut stream = tokio::io::BufReader::new(stream);

                let mut line_to_send = secured_input.iter().cloned();

                stream.write_all(line_to_send.next().unwrap().as_bytes()).await.unwrap();

                loop {
                    let mut line_received = String::new();
                    // read until '\n' or '\r\n'
                    if stream.read_line(&mut line_received).await.map_or(true, |l| l == 0) {
                        break;
                    }

                    output.push(line_received);
                    if output.last().unwrap().chars().nth(3) == Some('-') { continue; }
                    match line_to_send.next() {
                        Some(line) => stream.write_all(line.as_bytes()).await.unwrap(),
                        None => break,
                    }
                }
            )?

            output
        });

        let (client, server) = tokio::join!(client, server);
        let (client, _server) = (client.unwrap(), server.unwrap());

        pretty_assertions::assert_eq!(expected, client);

        queue_manager_cloned
    }};
    (
        fn $name:ident,
        input = $input:expr,
        expected = $expected:expr
        $(, tunnel = $server_name_tunnel:expr)?
        $(, config = $config:expr)?
        $(, config_arc = $config_arc:expr)?
        $(, mail_handler = $mail_handler:expr)?
        $(, hierarchy_builder = $hierarchy_builder:expr)?
        $(,)?
    ) => {
        #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
        async fn $name() {
            run_pipelined_test! {
                input = $input,
                expected = $expected
                $(, tunnel = $server_name_tunnel)?
                $(, config = $config)?
                $(, config_arc = $config_arc)?
                $(, mail_handler = $mail_handler)?
                $(, hierarchy_builder = $hierarchy_builder)?
            };
        }
    };
}
