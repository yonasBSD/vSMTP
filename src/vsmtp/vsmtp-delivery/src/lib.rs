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

//! vSMTP delivery system

#![doc(html_no_source)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]
//
#![warn(rust_2018_idioms)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]

/// a few helpers to create systems that will deliver emails.
pub mod transport {
    use trust_dns_resolver::TokioAsyncResolver;
    use vsmtp_common::{
        mail_context::{Finished, MailContext},
        rcpt::Rcpt,
        Address,
    };
    use vsmtp_config::Config;

    ///
    #[async_trait::async_trait]
    pub trait Transport {
        /// Take the data required to deliver the email and return the updated version of the recipient.
        async fn deliver(
            self,
            config: &Config,
            context: &MailContext<Finished>,
            from: &Address,
            to: Vec<Rcpt>,
            content: &str,
        ) -> Vec<Rcpt>;
    }

    mod deliver;
    mod forward;
    mod maildir;
    mod mbox;

    pub use deliver::Deliver;
    pub use forward::Forward;
    pub use maildir::Maildir;
    pub use mbox::MBox;

    /// no transfer will be made if this resolver is selected.
    pub struct NoTransfer;
    use anyhow::Context;

    #[async_trait::async_trait]
    impl Transport for NoTransfer {
        async fn deliver(
            self,
            _: &Config,
            _: &MailContext<Finished>,
            _: &Address,
            to: Vec<Rcpt>,
            _: &str,
        ) -> Vec<Rcpt> {
            to
        }
    }

    /// build a transport using opportunistic tls and toml specified certificates.
    /// TODO: resulting transport should be cached.
    fn build_transport(
        config: &Config,
        // will be used for tlsa record resolving.
        _: &TokioAsyncResolver,
        from: &vsmtp_common::Address,
        target: &str,
        port: Option<u16>,
    ) -> anyhow::Result<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>> {
        let tls_builder =
            lettre::transport::smtp::client::TlsParameters::builder(target.to_string());

        // from's domain could match the root domain of the server.
        let tls_parameters =
            if config.server.domain == from.domain() && config.server.tls.is_some() {
                tls_builder.add_root_certificate(
                    lettre::transport::smtp::client::Certificate::from_der(
                        config
                            .server
                            .tls
                            .as_ref()
                            .unwrap()
                            .certificate
                            .inner
                            .0
                            .clone(),
                    )
                    .context("failed to parse certificate as der")?,
                )
            }
            // or a domain from one of the virtual domains.
            else if let Some(tls_config) = config
                .server
                .r#virtual
                .get(from.domain())
                .and_then(|domain| domain.tls.as_ref())
            {
                tls_builder.add_root_certificate(
                    lettre::transport::smtp::client::Certificate::from_der(
                        tls_config.certificate.inner.0.clone(),
                    )
                    .context("failed to parse certificate as der")?,
                )
            // if not, no certificate are used.
            } else {
                tls_builder
            }
            .build_rustls()
            .context("failed to build tls parameters")?;

        Ok(
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(target)
                .hello_name(lettre::transport::smtp::extension::ClientId::Domain(
                    from.domain().to_string(),
                ))
                .port(port.unwrap_or(lettre::transport::smtp::SMTP_PORT))
                .tls(lettre::transport::smtp::client::Tls::Opportunistic(
                    tls_parameters,
                ))
                .build(),
        )
    }
}
