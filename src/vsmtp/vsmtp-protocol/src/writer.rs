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
use crate::{receiver::ErrorCounter, ReceiverContext, ReceiverHandler, Verb};
use tokio::io::AsyncWriteExt;
use vsmtp_common::Reply;

/// writer used for pipelining
/// it keep a buffer of answers
#[allow(clippy::module_name_repetitions)]
pub struct WindowWriter<W: tokio::io::AsyncWrite + Unpin + Send> {
    inner: W,
    buffer: Vec<Reply>,
}

impl<W: tokio::io::AsyncWrite + Unpin + Send> AsMut<W> for WindowWriter<W> {
    #[inline]
    fn as_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: tokio::io::AsyncWrite + Unpin + Send> WindowWriter<W> {
    // Create a new WindowWriter
    #[inline]
    #[must_use]
    pub const fn new(inner: W) -> Self {
        Self {
            inner,
            buffer: Vec::<Reply>::new(),
        }
    }

    /// Consume the instance and return the underlying writer.
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// check if the internal writer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Send the buffer to the client.
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] produced by the underlying writer
    #[inline]
    pub async fn write_all(&mut self, buffer: &str) -> std::io::Result<()> {
        tracing::trace!(">> {:?}", buffer);
        self.write_all_bytes(buffer.as_bytes()).await
    }

    /// Send the buffer to the client.
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] produced by the underlying writer
    #[inline]
    pub async fn write_all_bytes(&mut self, buffer: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(buffer).await
    }

    /// update error counters and return appropriate message based on these counters.
    async fn handle_error<T: ReceiverHandler + Send>(
        &mut self,
        ctx: &mut ReceiverContext,
        error_counter: &mut ErrorCounter,
        handler: &mut T,
        reply: Reply,
    ) -> Reply {
        if !reply.code().is_error() {
            return reply;
        }
        error_counter.error_count += 1;

        let hard_error = error_counter.threshold_hard_error;
        let soft_error = error_counter.threshold_soft_error;

        if hard_error != -1 && error_counter.error_count >= hard_error {
            return handler.on_hard_error(ctx, reply).await;
        }
        if soft_error != -1 && error_counter.error_count >= soft_error {
            return handler.on_soft_error(ctx, reply).await;
        }
        reply
    }

    pub async fn direct_send_reply<T: ReceiverHandler + Send>(
        &mut self,
        ctx: &mut ReceiverContext,
        error_counter: &mut ErrorCounter,
        handler: &mut T,
        reply: Reply,
    ) -> std::io::Result<()> {
        let final_reply = self.handle_error(ctx, error_counter, handler, reply).await;
        self.write_all(final_reply.as_ref()).await
    }

    /// analyze the message if it can be stored in a buffer. It is send directly otherwise.
    pub async fn send_reply<T: ReceiverHandler + Send>(
        &mut self,
        ctx: &mut ReceiverContext,
        error_counter: &mut ErrorCounter,
        handler: &mut T,
        reply: Reply,
        verb: Verb,
    ) -> std::io::Result<()> {
        let final_reply = self.handle_error(ctx, error_counter, handler, reply).await;
        if verb.is_bufferable() {
            if !self.buffer.is_empty() {
                self.flush().await?;
            }
            return self.write_all(final_reply.as_ref()).await;
        }
        self.buffer.push(final_reply);
        Ok(())
    }

    /// send all buffered response in one go.
    pub async fn flush(&mut self) -> std::io::Result<()> {
        let full_response: Vec<String> = self
            .buffer
            .clone()
            .into_iter()
            .map(|r| r.to_string())
            .collect();
        self.write_all(full_response.concat().as_str()).await?;
        self.buffer.clear();
        Ok(())
    }
}

/// Sink for sending reply to the client
pub struct Writer<W: tokio::io::AsyncWrite + Unpin + Send> {
    inner: W,
}

impl<W: tokio::io::AsyncWrite + Unpin + Send> AsMut<W> for Writer<W> {
    #[inline]
    fn as_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: tokio::io::AsyncWrite + Unpin + Send> Writer<W> {
    /// Create a new instance
    #[inline]
    #[must_use]
    pub const fn new(inner: W) -> Self {
        Self { inner }
    }

    /// Consume the instance and return the underlying writer.
    #[inline]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Send the buffer to the client.
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] produced by the underlying writer
    #[inline]
    pub async fn write_all(&mut self, buffer: &str) -> std::io::Result<()> {
        tracing::trace!(">> {:?}", buffer);
        self.write_all_bytes(buffer.as_bytes()).await
    }

    /// Send the buffer to the client.
    ///
    /// # Errors
    ///
    /// * [`std::io::Error`] produced by the underlying writer
    #[inline]
    pub async fn write_all_bytes(&mut self, buffer: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(buffer).await
    }

    /// # Errors
    ///
    /// * [`std::io::Error`] produced by the underlying writer
    #[inline]
    pub async fn send_reply<T: ReceiverHandler + Send>(
        &mut self,
        ctx: &mut ReceiverContext,
        error_counter: &mut ErrorCounter,
        handler: &mut T,
        reply: Reply,
    ) -> std::io::Result<()> {
        if !reply.code().is_error() {
            return self.write_all(reply.as_ref()).await;
        }
        error_counter.error_count += 1;

        let hard_error = error_counter.threshold_hard_error;
        let soft_error = error_counter.threshold_soft_error;

        if hard_error != -1 && error_counter.error_count >= hard_error {
            let reply = handler.on_hard_error(ctx, reply).await;
            return self.write_all(reply.as_ref()).await;
        }

        if soft_error != -1 && error_counter.error_count >= soft_error {
            let reply = handler.on_soft_error(ctx, reply).await;
            return self.write_all(reply.as_ref()).await;
        }

        self.write_all(reply.as_ref()).await
    }
}
