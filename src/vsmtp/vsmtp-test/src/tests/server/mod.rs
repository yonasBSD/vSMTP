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

use crate::config;
use vsmtp_config::DnsResolvers;
use vsmtp_rule_engine::RuleEngine;
use vsmtp_server::{socket_bind_anyhow, Server};

macro_rules! listen_with {
    ($addr:expr, $addr_submission:expr, $addr_submissions:expr, $timeout:expr, $client_count_max:expr) => {{
        let config = std::sync::Arc::new({
            let mut config = config::local_test();
            config.server.interfaces.addr = $addr;
            config.server.interfaces.addr_submission = $addr_submission;
            config.server.interfaces.addr_submissions = $addr_submissions;
            config.server.client_count_max = $client_count_max;
            config
        });

        let queue_manager = <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(
            config.clone(),
            vec![],
        )
        .unwrap();
        let (emitter, _working, _delivery) = vsmtp_server::scheduler::init(
            config.server.queues.working.channel_size,
            config.server.queues.delivery.channel_size,
        );
        let resolvers = std::sync::Arc::new(DnsResolvers::from_config(&config).unwrap());

        let s = Server::new(
            config.clone(),
            std::sync::Arc::new(
                RuleEngine::new(config.clone(), resolvers, queue_manager.clone()).unwrap(),
            ),
            queue_manager,
            emitter,
        )
        .unwrap();

        tokio::time::timeout(
            std::time::Duration::from_millis($timeout),
            s.listen((
                config
                    .server
                    .interfaces
                    .addr
                    .iter()
                    .cloned()
                    .map(socket_bind_anyhow)
                    .collect::<anyhow::Result<Vec<std::net::TcpListener>>>()
                    .unwrap(),
                config
                    .server
                    .interfaces
                    .addr_submission
                    .iter()
                    .cloned()
                    .map(socket_bind_anyhow)
                    .collect::<anyhow::Result<Vec<std::net::TcpListener>>>()
                    .unwrap(),
                config
                    .server
                    .interfaces
                    .addr_submissions
                    .iter()
                    .cloned()
                    .map(socket_bind_anyhow)
                    .collect::<anyhow::Result<Vec<std::net::TcpListener>>>()
                    .unwrap(),
            )),
        )
        .await
        .unwrap_err();
    }};
}

#[tokio::test]
async fn basic() {
    listen_with![
        vec!["0.0.0.0:10021".parse().unwrap()],
        vec!["0.0.0.0:10588".parse().unwrap()],
        vec!["0.0.0.0:10466".parse().unwrap()],
        10,
        1
    ];
}

#[ignore]
#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 8))]
async fn one_client_max_ok() {
    let server = tokio::spawn(async move {
        listen_with![
            vec!["127.0.0.1:10016".parse().unwrap()],
            vec!["127.0.0.1:10578".parse().unwrap()],
            vec!["127.0.0.1:10456".parse().unwrap()],
            500,
            1
        ];
    });

    let client = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let mail = lettre::Message::builder()
            .from("NoBody <nobody@domain.tld>".parse().unwrap())
            .reply_to("Yuin <yuin@domain.tld>".parse().unwrap())
            .to("Hei <hei@domain.tld>".parse().unwrap())
            .subject("Happy new year")
            .body(String::from("Be happy!"))
            .unwrap();

        let sender =
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous("127.0.0.1")
                .port(10016)
                .build();

        lettre::AsyncTransport::send(&sender, mail).await
    });

    let (client, server) = tokio::join!(client, server);
    server.unwrap();

    dbg!(client
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .message()
        .collect::<Vec<_>>());

    assert_eq!(client.unwrap().unwrap().message().next().unwrap(), "Ok");
}

// FIXME: randomly fail the CI
/*
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn one_client_max_err() {
    let server = tokio::spawn(async move {
        listen_with![
            vec!["127.0.0.1:10006".parse().unwrap()],
            vec!["127.0.0.1:10568".parse().unwrap()],
            vec!["127.0.0.1:10446".parse().unwrap()],
            1000,
            1
        ];
    });

    let now = tokio::time::Instant::now();
    let until = now
        .checked_add(std::time::Duration::from_millis(100))
        .unwrap();

    let client = tokio::spawn(async move {
        tokio::time::sleep_until(until).await;
        let mail = lettre::Message::builder()
            .from("NoBody <nobody@domain.tld>".parse().unwrap())
            .reply_to("Yuin <yuin@domain.tld>".parse().unwrap())
            .to("Hei <hei@domain.tld>".parse().unwrap())
            .subject("Happy new year")
            .body(String::from("Be happy!"))
            .unwrap();

        let sender = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
            "127.0.0.1",
        )
        .port(10006)
        .build();

        lettre::AsyncTransport::send(&sender, mail).await
    });

    let client2 = tokio::spawn(async move {
        tokio::time::sleep_until(until).await;
        let mail = lettre::Message::builder()
            .from("NoBody <nobody2@domain.tld>".parse().unwrap())
            .reply_to("Yuin <yuin@domain.tld>".parse().unwrap())
            .to("Hei <hei@domain.tld>".parse().unwrap())
            .subject("Happy new year")
            .body(String::from("Be happy!"))
            .unwrap();

        let sender = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
            "127.0.0.1",
        )
        .port(10006)
        .build();

        lettre::AsyncTransport::send(&sender, mail).await
    });

    let (server, client, client2) = tokio::join!(server, client, client2);
    server.unwrap();

    let client1 = format!("{}", client.unwrap().unwrap_err());
    let client2 = format!("{}", client2.unwrap().unwrap_err());

    // one of the client has been denied on connection, but we cant know which one
    let ok1_failed2 = client1
        == "permanent error (554): permanent problems with the remote server"
        && client2 == "permanent error (554): Cannot process connection, closing";
    let ok2_failed1 = client2
        == "permanent error (554): permanent problems with the remote server"
        && client1 == "permanent error (554): Cannot process connection, closing";

    assert!(ok1_failed2 || ok2_failed1);
}
*/
