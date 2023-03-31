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
use tokio_stream::StreamExt;
use vqueue::{GenericQueueManager, QueueID};
use vsmtp_config::DnsResolvers;
use vsmtp_rule_engine::{ExecutionStage, RuleEngine};
use vsmtp_server::{scheduler, working::handle_one, ProcessMessage};

#[test_log::test(tokio::test)]
async fn cannot_deserialize() {
    let config = local_test();

    let (emitter, _working, _delivery) = scheduler::init(
        config.server.queues.working.channel_size,
        config.server.queues.delivery.channel_size,
    );
    let config = std::sync::Arc::new(config);

    let resolvers = std::sync::Arc::new(DnsResolvers::from_config(&config).unwrap());
    let queue_manager =
        <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![])
            .unwrap();

    handle_one(
        std::sync::Arc::new(
            RuleEngine::with_hierarchy(
                |builder| Ok(builder.add_root_filter_rules("#{}")?.build()),
                config,
                resolvers.clone(),
                queue_manager.clone(),
            )
            .unwrap(),
        ),
        queue_manager,
        ProcessMessage::new(uuid::Uuid::nil()),
        emitter,
    )
    .await
    .unwrap_err();
}

#[test_log::test(tokio::test)]
async fn basic() {
    let config = std::sync::Arc::new(local_test());
    let queue_manager =
        <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![])
            .unwrap();

    let mut ctx = local_ctx();
    let message_uuid = uuid::Uuid::new_v4();
    ctx.mail_from.message_uuid = message_uuid;
    queue_manager
        .write_both(&QueueID::Working, &ctx, &local_msg())
        .await
        .unwrap();

    let (emitter, _working, mut delivery) = scheduler::init(
        config.server.queues.working.channel_size,
        config.server.queues.delivery.channel_size,
    );
    let resolvers = std::sync::Arc::new(DnsResolvers::from_config(&config).unwrap());

    handle_one(
        std::sync::Arc::new(
            RuleEngine::with_hierarchy(
                |builder| {
                    Ok(builder
                        .add_root_filter_rules("#{}")?
                        .add_domain_rules("testserver.com".parse().unwrap())
                        .with_incoming("#{}")?
                        .with_outgoing("#{}")?
                        .with_internal("#{}")?
                        .build()
                        .build())
                },
                config.clone(),
                resolvers.clone(),
                queue_manager.clone(),
            )
            .unwrap(),
        ),
        queue_manager.clone(),
        ProcessMessage::new(message_uuid),
        emitter.clone(),
    )
    .await
    .unwrap();

    let delivery_recv = delivery.as_stream();
    tokio::pin!(delivery_recv);
    assert_eq!(*delivery_recv.next().await.unwrap().as_ref(), message_uuid);
    queue_manager
        .get_ctx(&QueueID::Working, &message_uuid)
        .await
        .unwrap_err();
    queue_manager
        .get_ctx(&QueueID::Deliver, &message_uuid)
        .await
        .unwrap();
}

#[test_log::test(tokio::test)]
async fn denied() {
    let config = std::sync::Arc::new(local_test());
    let queue_manager =
        <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![])
            .unwrap();

    let mut ctx = local_ctx();
    let message_uuid = uuid::Uuid::new_v4();

    ctx.mail_from.message_uuid = message_uuid;
    queue_manager
        .write_both(&QueueID::Working, &ctx, &local_msg())
        .await
        .unwrap();

    let (emitter, _working, _delivery) = scheduler::init(
        config.server.queues.working.channel_size,
        config.server.queues.delivery.channel_size,
    );
    let resolvers = std::sync::Arc::new(DnsResolvers::from_config(&config).unwrap());

    handle_one(
        std::sync::Arc::new(
            RuleEngine::with_hierarchy(
                |builder| {
                    Ok(builder
                        .add_root_filter_rules(&format!(
                            r#"#{{ {}: [ rule "abc" || state::deny(), ] }}"#,
                            ExecutionStage::PostQ
                        ))?
                        .build())
                },
                config.clone(),
                resolvers.clone(),
                queue_manager.clone(),
            )
            .unwrap(),
        ),
        queue_manager.clone(),
        ProcessMessage::new(message_uuid),
        emitter,
    )
    .await
    .unwrap();

    queue_manager
        .get_ctx(&QueueID::Working, &message_uuid)
        .await
        .unwrap_err();

    queue_manager
        .get_ctx(&QueueID::Dead, &message_uuid)
        .await
        .unwrap();
}

#[test_log::test(tokio::test)]
async fn quarantine() {
    let config = std::sync::Arc::new(local_test());
    let queue_manager =
        <vqueue::temp::QueueManager as vqueue::GenericQueueManager>::init(config.clone(), vec![])
            .unwrap();

    let mut ctx = local_ctx();
    let message_uuid = uuid::Uuid::new_v4();

    ctx.mail_from.message_uuid = message_uuid;
    queue_manager
        .write_both(&QueueID::Working, &ctx, &local_msg())
        .await
        .unwrap();

    let (emitter, _working, _delivery) = scheduler::init(
        config.server.queues.working.channel_size,
        config.server.queues.delivery.channel_size,
    );
    let resolvers = std::sync::Arc::new(DnsResolvers::from_config(&config).unwrap());

    let rules = format!(
        "#{{ {}: [ rule \"quarantine\" || state::quarantine(\"unit-test\") ] }}",
        ExecutionStage::PostQ
    );

    handle_one(
        std::sync::Arc::new(
            RuleEngine::with_hierarchy(
                move |builder| {
                    Ok(builder
                        .add_root_filter_rules(&rules)?
                        .add_domain_rules("testserver.com".parse().unwrap())
                        .with_incoming(&rules)?
                        .with_outgoing(&rules)?
                        .with_internal(&rules)?
                        .build()
                        .build())
                },
                config.clone(),
                resolvers.clone(),
                queue_manager.clone(),
            )
            .unwrap(),
        ),
        queue_manager.clone(),
        ProcessMessage::new(message_uuid),
        emitter,
    )
    .await
    .unwrap();

    queue_manager
        .get_ctx(
            &QueueID::Quarantine {
                name: "unit-test".to_string(),
            },
            &message_uuid,
        )
        .await
        .unwrap();

    queue_manager
        .get_ctx(&QueueID::Working, &message_uuid)
        .await
        .unwrap_err();
    queue_manager
        .get_ctx(&QueueID::Dead, &message_uuid)
        .await
        .unwrap_err();
}
