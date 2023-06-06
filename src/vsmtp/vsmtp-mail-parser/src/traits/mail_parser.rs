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
    message::{mail::Mail, raw_body::RawBody},
    ParserError, ParserResult,
};

/// An abstract mail parser
#[async_trait::async_trait]
pub trait MailParser: Default {
    /// From a buffer of strings, return either:
    ///
    /// * a RFC valid [`Mail`] object
    /// * a [`RawBody`] instance
    ///
    /// # Errors
    ///
    /// * the input is not compliant
    fn parse_sync(&mut self, raw: Vec<Vec<u8>>) -> ParserResult<either::Either<RawBody, Mail>>;

    /// Parses an email from a stream.
    ///
    /// # Args`
    ///
    /// * `stream`   - The stream to parse the email from.
    /// * `max_size` - The maximum size of the email defined by SIZE EHLO extension.
    ///                If set to 0, there are no size restrictions.
    ///
    /// # Errors
    ///
    /// * The input is not compliant
    /// * The message size exceeds the maximum size defined `max_size`.
    async fn parse<'a>(
        &'a mut self,
        mut stream: impl tokio_stream::Stream<Item = Result<Vec<u8>, ParserError>> + Unpin + Send + 'a,
        max_size: usize,
    ) -> ParserResult<either::Either<RawBody, Mail>> {
        let mut buffer = Vec::with_capacity(if max_size == 0 { 20_000_000 } else { max_size });
        let mut size = 0;

        while let Some(i) = tokio_stream::StreamExt::try_next(&mut stream).await? {
            size += i.len();
            buffer.push(i);
        }

        if max_size != 0 && size > max_size {
            return Err(ParserError::MailSizeExceeded {
                expected: max_size,
                got: size,
            });
        }

        self.parse_sync(buffer)
    }

    ///
    fn convert(mut self, input: &RawBody) -> ParserResult<Option<Mail>> {
        // TODO(perf):
        let raw = input.to_string();

        self.parse_sync(raw.lines().map(|l| l.as_bytes().to_vec()).collect())
            .map(|either| match either {
                either::Left(_) => None,
                either::Right(mail) => Some(mail),
            })
    }
}
