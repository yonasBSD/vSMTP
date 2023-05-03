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

use crate::Handler;
use tokio_rustls::rustls;
use vsmtp_common::{
    auth::{Credentials, Mechanism},
    status::Status,
    ClientName, Reply,
};
use vsmtp_mail_parser::MailParser;
use vsmtp_protocol::{
    AcceptArgs, AuthArgs, AuthError, CallbackWrap, ConnectionKind, EhloArgs, HeloArgs,
    ReceiverContext,
};
use vsmtp_rule_engine::{ExecutionStage, RuleEngine, RuleState};

impl<Parser, ParserFactory> Handler<Parser, ParserFactory>
where
    Parser: MailParser + Send + Sync,
    ParserFactory: Fn() -> Parser + Send + Sync,
{
    pub(super) fn on_accept_inner(
        &mut self,
        ctx: &mut ReceiverContext,
        args: &AcceptArgs,
    ) -> Reply {
        if self
            .rule_engine
            .get_delegation_directive_bound_to_address(args.server_addr)
            .is_some()
        {
            self.state
                .context()
                .write()
                .expect("bad state")
                .set_skipped(Status::DelegationResult);
            self.skipped = Some(Status::DelegationResult);
        }

        let reply =
            match self
                .rule_engine
                .run_when(&self.state, &mut self.skipped, ExecutionStage::Connect)
            {
                // FIXME: do we really want to let the end-user override the EHLO/HELO reply?
                Status::Faccept(reply) | Status::Accept(reply) => reply,
                Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                    format!("220 {} Service ready\r\n", self.config.server.name)
                        .parse::<Reply>()
                        .unwrap()
                }
                Status::Deny(reply) | Status::Reject(reply) => {
                    ctx.deny();
                    return reply;
                }
                // FIXME: user ran a delegate method before postq/delivery
                Status::Delegated(_) => unreachable!(),
            };

        // NOTE: in that case, the return value is ignored and
        // we have to manually trigger the TLS handshake,
        if args.kind == ConnectionKind::Tunneled
            && !self
                .state
                .context()
                .read()
                .expect("state poisoned")
                .is_secured()
        {
            match &self.rustls_config {
                Some(config) => ctx.upgrade_tls(config.clone(), std::time::Duration::from_secs(2)),
                None => ctx.deny(),
            }
            return "100 ignored value\r\n".parse().unwrap();
        }

        reply
    }

    pub(super) fn generate_sasl_callback_inner(&self) -> CallbackWrap {
        CallbackWrap(Box::new(RsaslSessionCallback {
            rule_engine: self.rule_engine.clone(),
            state: self.state.clone(),
        }))
    }

    pub(super) fn on_post_tls_handshake_inner(
        &mut self,
        sni: Option<String>,
        protocol_version: rustls::ProtocolVersion,
        cipher_suite: rustls::CipherSuite,
        peer_certificates: Option<Vec<rustls::Certificate>>,
        alpn_protocol: Option<Vec<u8>>,
    ) -> Reply {
        let server_name = sni.map(|sni| sni.parse().unwrap());

        self.state
            .context()
            .write()
            .expect("state poisoned")
            .to_secured(
                server_name.clone(),
                protocol_version,
                cipher_suite,
                peer_certificates,
                alpn_protocol,
            )
            .expect("bad state");

        format!(
            "220 {} Service ready\r\n",
            server_name.unwrap_or_else(|| self.config.server.name.clone())
        )
        .parse::<Reply>()
        .unwrap()
    }

    pub(super) fn on_starttls_inner(&mut self, ctx: &mut ReceiverContext) -> Reply {
        if self
            .state
            .context()
            .read()
            .expect("state poisoned")
            .is_secured()
        {
            "554 5.5.1 Error: TLS already active\r\n"
                .parse::<Reply>()
                .unwrap()
        } else {
            self.rustls_config.as_ref().map_or(
                "454 TLS not available due to temporary reason\r\n"
                    .parse::<Reply>()
                    .unwrap(),
                |config| {
                    ctx.upgrade_tls(config.clone(), std::time::Duration::from_secs(2));
                    "220 TLS go ahead\r\n".parse::<Reply>().unwrap()
                },
            )
        }
    }

    pub(super) fn on_auth_inner(
        &mut self,
        ctx: &mut ReceiverContext,
        args: AuthArgs,
    ) -> Option<Reply> {
        if let Some(auth) = &self.config.server.smtp.auth {
            if !self
                .state
                .context()
                .read()
                .expect("state poisoned")
                .is_secured()
                && args.mechanism.must_be_under_tls()
                && !auth.enable_dangerous_mechanism_in_clair
            {
                return Some(
                    "538 5.7.11 Encryption required for requested authentication mechanism\r\n"
                        .parse::<Reply>()
                        .unwrap(),
                );
            }

            ctx.authenticate(args.mechanism, args.initial_response);

            None
        } else {
            Some("502 Command not implemented\r\n".parse::<Reply>().unwrap())
        }
    }

    pub(super) fn on_post_auth_inner(
        &mut self,
        ctx: &mut ReceiverContext,
        result: Result<(), AuthError>,
    ) -> Reply {
        match result {
            Ok(()) => {
                self.state
                    .context()
                    .write()
                    .expect("state poisoned")
                    .auth_mut()
                    .expect("bad state")
                    .authenticated = true;

                "235 2.7.0 Authentication succeeded\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::ClientMustNotStart) => {
                "501 5.7.0 Client must not start with this mechanism\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::ValidationError(..)) => {
                ctx.deny();
                "535 5.7.8 Authentication credentials invalid\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::Canceled) => {
                let state = self.state.context();
                let mut guard = state.write().expect("state poisoned");
                let auth_properties = guard.to_auth().expect("bad state");

                auth_properties.cancel_count += 1;
                let attempt_count_max = self
                    .config
                    .server
                    .smtp
                    .auth
                    .as_ref()
                    .map_or(-1, |auth| auth.attempt_count_max);

                if attempt_count_max != -1
                    && auth_properties.cancel_count >= attempt_count_max.try_into().unwrap()
                {
                    ctx.deny();
                }

                "501 Authentication canceled by client\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::Base64 { .. }) => "501 5.5.2 Invalid, not base64\r\n"
                .parse::<Reply>()
                .unwrap(),
            Err(AuthError::SessionError(e)) => {
                tracing::warn!(%e, "auth error");
                ctx.deny();
                "454 4.7.0 Temporary authentication failure\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::IO(e)) => todo!("{e}"),
            Err(AuthError::ConfigError(rsasl::prelude::SASLError::NoSharedMechanism)) => {
                ctx.deny();
                "504 5.5.4 Mechanism is not supported\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::ConfigError(e)) => todo!("handle non_exhaustive pattern: {e}"),
        }
    }

    pub(super) fn on_helo_inner(&mut self, ctx: &mut ReceiverContext, args: HeloArgs) -> Reply {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .to_helo(ClientName::Domain(args.client_name), true)
            .expect("bad state");

        match self
            .rule_engine
            .run_when(&self.state, &mut self.skipped, ExecutionStage::Helo)
        {
            Status::Faccept(reply) | Status::Accept(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                "250 Ok\r\n".parse::<Reply>().unwrap()
            }
            Status::Deny(reply) | Status::Reject(reply) => {
                ctx.deny();
                reply
            }
            // FIXME: user ran a delegate method before postq/delivery
            Status::Delegated(_) => unreachable!(),
        }
    }

    pub(super) fn on_ehlo_inner(&mut self, ctx: &mut ReceiverContext, args: EhloArgs) -> Reply {
        let vsl_ctx = self.state.context();

        vsl_ctx
            .write()
            .expect("state poisoned")
            .to_helo(args.client_name, false)
            .expect("bad state");

        match self
            .rule_engine
            .run_when(&self.state, &mut self.skipped, ExecutionStage::Helo)
        {
            Status::Faccept(reply) | Status::Accept(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                let ctx = vsl_ctx.read().expect("state poisoned");

                let auth_mechanism_list: Option<(Vec<Mechanism>, Vec<Mechanism>)> = self
                    .config
                    .server
                    .smtp
                    .auth
                    .as_ref()
                    .map(|auth| auth.mechanisms.iter().partition(|m| m.must_be_under_tls()));

                if ctx.is_secured() {
                    [
                        Some(format!("250-{}\r\n", ctx.server_name())),
                        auth_mechanism_list.as_ref().map(|(must_be_secured, _)| {
                            format!(
                                "250-AUTH {}\r\n",
                                must_be_secured
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )
                        }),
                        Some("250-8BITMIME\r\n".to_string()),
                        Some("250 SMTPUTF8\r\n".to_string()),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<String>()
                    .parse::<Reply>()
                    .unwrap()
                } else {
                    [
                        Some(format!("250-{}\r\n", &ctx.server_name())),
                        auth_mechanism_list.as_ref().map(|(plain, secured)| {
                            if self
                                .config
                                .server
                                .smtp
                                .auth
                                .as_ref()
                                .map_or(false, |auth| auth.enable_dangerous_mechanism_in_clair)
                            {
                                format!(
                                    "250-AUTH {}\r\n",
                                    &[secured.clone(), plain.clone()]
                                        .concat()
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                )
                            } else {
                                format!(
                                    "250-AUTH {}\r\n",
                                    secured
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                )
                            }
                        }),
                        Some("250-STARTTLS\r\n".to_string()),
                        Some("250-8BITMIME\r\n".to_string()),
                        Some("250 SMTPUTF8\r\n".to_string()),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<String>()
                    .parse::<Reply>()
                    .unwrap()
                }
            }
            Status::Deny(reply) | Status::Reject(reply) => {
                ctx.deny();
                reply
            }
            // FIXME: user ran a delegate method before postq/delivery
            Status::Delegated(_) => unreachable!(),
        }
    }
}

///
pub struct ValidationVSL;

impl rsasl::validate::Validation for ValidationVSL {
    type Value = ();
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error(
        "the rules at stage '{}' returned non '{}' status",
        ExecutionStage::Authenticate,
        Status::Accept("250 Ok\r\n".parse::<Reply>().unwrap()).as_ref()
    )]
    NonAcceptCode,
}

struct RsaslSessionCallback {
    rule_engine: std::sync::Arc<RuleEngine>,
    state: std::sync::Arc<RuleState>,
}

impl RsaslSessionCallback {
    #[allow(clippy::unnecessary_wraps)]
    fn inner_validate(
        &self,
        credentials: Credentials,
    ) -> Result<<ValidationVSL as rsasl::validate::Validation>::Value, ValidationError> {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .with_credentials(credentials)
            .expect("bad state");

        let mut skipped = None;
        let result =
            self.rule_engine
                .run_when(&self.state, &mut skipped, ExecutionStage::Authenticate);

        if !matches!(result, Status::Accept(..)) {
            return Err(ValidationError::NonAcceptCode);
        }

        Ok(())
    }
}

impl rsasl::callback::SessionCallback for RsaslSessionCallback {
    fn callback(
        &self,
        _session_data: &rsasl::callback::SessionData,
        _context: &rsasl::callback::Context<'_>,
        _request: &mut rsasl::callback::Request<'_>,
    ) -> Result<(), rsasl::prelude::SessionError> {
        Ok(())
    }

    fn validate(
        &self,
        session_data: &rsasl::callback::SessionData,
        context: &rsasl::callback::Context<'_>,
        validate: &mut rsasl::validate::Validate<'_>,
    ) -> Result<(), rsasl::validate::ValidationError> {
        let credentials = Credentials::try_from((session_data, context)).map_err(|e| match e {
            vsmtp_common::auth::Error::MissingField => {
                rsasl::validate::ValidationError::MissingRequiredProperty
            }
            otherwise => rsasl::validate::ValidationError::Boxed(Box::new(otherwise)),
        })?;

        validate.with::<ValidationVSL, _>(|| {
            self.inner_validate(credentials)
                .map_err(|e| rsasl::validate::ValidationError::Boxed(Box::new(e)))
        })?;

        Ok(())
    }
}
