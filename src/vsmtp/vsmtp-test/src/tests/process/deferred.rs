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

use crate::config::{local_ctx, local_msg, local_test};
use vqueue::{GenericQueueManager, QueueID};
use vsmtp_common::{
    transfer::Status,
    transport::{AbstractTransport, WrapperSerde},
};
use vsmtp_config::DnsResolvers;
use vsmtp_delivery::{Deliver, Forward, MBox, Maildir};
use vsmtp_server::{delivery::deferred::handle_one, ProcessMessage};

#[tokio::test]
async fn move_to_deferred() {
    let config = std::sync::Arc::new(local_test());
    let queue_manager = <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(
        config.clone(),
        vec![
            Deliver::get_symbol(),
            Forward::get_symbol(),
            Maildir::get_symbol(),
            MBox::get_symbol(),
        ],
    )
    .unwrap();
    let resolvers = std::sync::Arc::new(DnsResolvers::from_config(&config).unwrap());

    let mut ctx = local_ctx();
    let message_uuid = uuid::Uuid::new_v4();
    ctx.mail_from.message_uuid = message_uuid;
    ctx.rcpt_to
        .delivery
        .entry(WrapperSerde::Ready(std::sync::Arc::new(Deliver::new(
            resolvers.get_resolver_root(),
            config.clone(),
        ))))
        .and_modify(|rcpt| {
            rcpt.push(("test@localhost".parse().unwrap(), Status::default()));
        })
        .or_insert_with(|| vec![("test@localhost".parse().unwrap(), Status::default())]);

    queue_manager
        .write_both(&QueueID::Deferred, &ctx, &local_msg())
        .await
        .unwrap();

    handle_one(
        config.clone(),
        queue_manager.clone(),
        ProcessMessage::new(message_uuid),
        time::OffsetDateTime::UNIX_EPOCH,
    )
    .await
    .unwrap();

    queue_manager
        .get_ctx(&QueueID::Deliver, &message_uuid)
        .await
        .unwrap_err();
    queue_manager
        .get_ctx(&QueueID::Dead, &message_uuid)
        .await
        .unwrap_err();

    queue_manager
        .get_ctx(&QueueID::Deferred, &message_uuid)
        .await
        .unwrap();
}

#[tokio::test]
async fn move_to_dead() {
    let config = std::sync::Arc::new(local_test());
    let queue_manager =
        <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![])
            .unwrap();

    let mut ctx = local_ctx();
    let message_uuid = uuid::Uuid::new_v4();
    ctx.mail_from.message_uuid = message_uuid;

    queue_manager
        .write_both(&QueueID::Deferred, &ctx, &local_msg())
        .await
        .unwrap();

    handle_one(
        config.clone(),
        queue_manager.clone(),
        ProcessMessage::new(message_uuid),
        time::OffsetDateTime::UNIX_EPOCH,
    )
    .await
    .unwrap();

    queue_manager
        .get_ctx(&QueueID::Deferred, &message_uuid)
        .await
        .unwrap_err();

    queue_manager
        .get_ctx(&QueueID::Dead, &message_uuid)
        .await
        .unwrap();
}
