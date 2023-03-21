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
use crate::{delegate, delivery::add_trace_information, ProcessMessage};
use anyhow::Context;
use vqueue::{GenericQueueManager, QueueID};
use vsmtp_common::{
    status,
    transfer::{self, error::Rule},
};
use vsmtp_config::Config;
use vsmtp_delivery::{split_and_sort_and_send, SenderOutcome};
use vsmtp_rule_engine::{ExecutionStage, RuleEngine};

pub(crate) async fn flush_deliver_queue<Q: GenericQueueManager + Sized + 'static>(
    config: std::sync::Arc<Config>,
    queue_manager: std::sync::Arc<Q>,
    rule_engine: std::sync::Arc<RuleEngine>,
) {
    // FIXME: add span on the function.
    tracing::info!("Flushing deliver queue.");

    let queued = match queue_manager.list(&QueueID::Deliver).await {
        Ok(queued) => queued,
        Err(error) => {
            tracing::error!(%error, "Flushing failed");
            return;
        }
    };

    for i in queued {
        let message_uuid = match i.map(|i| <uuid::Uuid as std::str::FromStr>::from_str(&i)) {
            Ok(Ok(message_uuid)) => message_uuid,
            Ok(Err(error)) => {
                tracing::error!(%error, "Invalid message id in deliver queue.");
                continue;
            }
            Err(error) => {
                tracing::error!(%error, "Deliver message id missing.");
                continue;
            }
        };

        let _err = handle_one(
            config.clone(),
            queue_manager.clone(),
            ProcessMessage::new(message_uuid),
            rule_engine.clone(),
        )
        .await;
    }
}

/// Handle one message in the delivery queue.
#[allow(clippy::too_many_lines)]
#[tracing::instrument(name = "delivery", skip_all, err(Debug), fields(uuid = %process_message.as_ref()))]
pub async fn handle_one<Q: GenericQueueManager + Sized + 'static>(
    config: std::sync::Arc<Config>,
    queue_manager: std::sync::Arc<Q>,
    process_message: ProcessMessage,
    rule_engine: std::sync::Arc<RuleEngine>,
) -> anyhow::Result<()> {
    let queue = if process_message.is_from_delegation() {
        QueueID::Delegated
    } else {
        QueueID::Deliver
    };

    let (ctx, msg) = queue_manager
        .get_both(&queue, process_message.as_ref())
        .await?;

    let mut skipped = ctx.connect.skipped.clone();
    let (ctx, mut msg, result) = rule_engine.just_run_when(
        &mut skipped,
        ExecutionStage::Delivery,
        vsmtp_common::Context::Finished(ctx),
        msg,
    );

    let mut ctx = ctx.unwrap_finished().context("context is not finished")?;

    match &skipped {
        Some(status @ status::Status::Quarantine(path)) => {
            queue_manager
                .move_to(&queue, &QueueID::Quarantine { name: path.into() }, &ctx)
                .await?;

            queue_manager
                .write_msg(process_message.as_ref(), &msg)
                .await?;

            tracing::warn!(status = status.as_ref(), "Rules skipped.");

            return Ok(());
        }
        Some(status @ status::Status::Delegated(delegator)) => {
            ctx.connect.skipped = Some(status::Status::DelegationResult);

            queue_manager
                .move_to(&queue, &QueueID::Delegated, &ctx)
                .await?;

            queue_manager
                .write_msg(process_message.as_ref(), &msg)
                .await?;

            // NOTE: needs to be executed after writing, because the other
            //       thread could pickup the email faster than this function.
            delegate(delegator, &ctx, &msg)?;

            tracing::warn!(status = status.as_ref(), "Rules skipped.");

            return Ok(());
        }
        Some(status::Status::DelegationResult) => {
            anyhow::bail!(
                "delivery is the last stage, delegation results cannot travel down any further."
            )
        }
        Some(status::Status::Deny(code)) => {
            for rcpt in &mut ctx.rcpt_to.delivery.values_mut().flatten() {
                rcpt.1 = transfer::Status::failed(Rule::Denied(code.clone()));
            }

            queue_manager.move_to(&queue, &QueueID::Dead, &ctx).await?;

            queue_manager
                .write_msg(process_message.as_ref(), &msg)
                .await?;

            return Ok(());
        }
        Some(reason) => {
            tracing::warn!(status = ?reason, "Rules skipped.");
        }
        None => {}
    };

    add_trace_information(&ctx, &mut msg, &result)?;

    match split_and_sort_and_send(config, &mut ctx, &msg).await {
        SenderOutcome::MoveToDead => {
            queue_manager.move_to(&queue, &QueueID::Dead, &ctx).await?;

            queue_manager
                .write_msg(process_message.as_ref(), &msg)
                .await
        }
        SenderOutcome::MoveToDeferred => {
            queue_manager
                .move_to(&queue, &QueueID::Deferred, &ctx)
                .await?;

            queue_manager
                .write_msg(process_message.as_ref(), &msg)
                .await
        }
        SenderOutcome::RemoveFromDisk => {
            queue_manager
                .remove_both(&queue, process_message.as_ref())
                .await
        }
    }
}
