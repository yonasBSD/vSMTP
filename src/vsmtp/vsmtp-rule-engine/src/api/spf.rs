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
    api::{
        EngineResult, {Context, Server},
    },
    error::RuntimeError,
};
use rhai::plugin::{
    mem, Dynamic, FnAccess, FnNamespace, Module, NativeCallContext, PluginFunction, RhaiResult,
    TypeId,
};
use vsmtp_auth::viaspf;
use vsmtp_common::ClientName;

const AUTH_HEADER: &str = "Authentication-Results";
const SPF_HEADER: &str = "Received-SPF";

pub use spf::*;

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Policy {
    #[default]
    Strict,
    Soft,
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Headers {
    Spf,
    Auth,
    #[default]
    Both,
    None,
}

#[derive(Default, serde::Deserialize)]
struct SpfParameters {
    #[serde(default)]
    header: Headers,
    #[serde(default)]
    policy: Policy,
}

/// Implementation of the Sender Policy Framework (SPF), described by RFC 4408. (<https://www.ietf.org/rfc/rfc4408.txt>)
#[rhai::plugin::export_module]
mod spf {
    use crate::api::{message::Impl, state};
    use vsmtp_common::status::Status;

    use crate::get_global;

    /// Check spf record following the Sender Policy Framework (RFC 7208).
    /// see <https://datatracker.ietf.org/doc/html/rfc7208>
    ///
    /// # Args
    ///
    /// * a map composed of the following parameters:
    ///     * `header` - The header(s) where the spf results will be written.
    ///                  Can be "spf", "auth", "both" or "none". (default: "both")
    ///     * `policy` - Degrees of flexibility when getting spf results.
    ///                  Can be "strict" or "soft". (default: "strict")
    ///                  A "soft" policy will let softfail pass while a "strict"
    ///                  policy will return a deny if the results are not "pass".
    ///
    /// # Return
    ///
    /// * `deny(code550_7_23 | code550_7_24)` - an error occurred during lookup. (returned even when a softfail is received using the "strict" policy)
    /// * `next()` - the operation succeeded.
    ///
    /// # Effective smtp stage
    ///
    /// `mail` and onwards.
    ///
    /// # Errors
    ///
    /// * The `header` argument is not valid.
    /// * The `policy` argument is not valid.
    ///
    /// # Note
    ///
    /// `spf::check` only checks for the sender's identity, not the `helo` value.
    ///
    /// # Example
    ///
    /// ```
    /// # let rules = r#"#{
    ///     mail: [
    ///        rule "check spf" || spf::check(),
    ///     ]
    /// }
    /// # "#;
    ///
    /// # let states = vsmtp_test::vsl::run(|builder| Ok(builder
    /// #   .add_root_filter_rules("#{}")?
    /// #      .add_domain_rules("testserver.com".parse().unwrap())
    /// #        .with_incoming(rules)?
    /// #        .with_outgoing(rules)?
    /// #        .with_internal(rules)?
    /// #      .build()
    /// #   .build()));
    /// # use vsmtp_common::{status::Status};
    /// # use vsmtp_rule_engine::ExecutionStage;
    /// # // NOTE: only testing parameter parsing here.
    /// # assert_eq!(states[&ExecutionStage::MailFrom].2,
    /// #   Status::Deny(
    /// #     "550 5.7.23 SPF validation failed\r\n".parse().unwrap(),
    /// #   )
    /// # );
    /// ```
    ///
    /// ```
    /// # let rules = r#"#{
    ///     mail: [
    ///         // if this check succeed, it wil return `next`.
    ///         // if it fails, it might return `deny` with a custom code
    ///         // (X.7.24 or X.7.25 for example)
    ///         //
    ///         // if you want to use the return status, just put the spf::check
    ///         // function on the last line of your rule.
    ///         rule "check spf 1" || {
    ///             log("debug", `running sender policy framework on ${ctx::mail_from()} identity ...`);
    ///             spf::check(#{ header: "spf", policy: "soft" })
    ///         },
    ///
    ///         // policy is set to "strict" by default.
    ///         rule "check spf 2" || spf::check(#{ header: "both" }),
    ///     ],
    /// }
    /// # "#;
    ///
    /// # let states = vsmtp_test::vsl::run(|builder| Ok(builder
    /// #   .add_root_filter_rules("#{}")?
    /// #      .add_domain_rules("testserver.com".parse().unwrap())
    /// #        .with_incoming(rules)?
    /// #        .with_outgoing(rules)?
    /// #        .with_internal(rules)?
    /// #      .build()
    /// #   .build()));
    /// # use vsmtp_common::{status::Status};
    /// # use vsmtp_rule_engine::ExecutionStage;
    /// # // NOTE: only testing parameter parsing here.
    /// # assert_eq!(states[&ExecutionStage::MailFrom].2,
    /// #   Status::Deny(
    /// #     "550 5.7.23 SPF validation failed\r\n".parse().unwrap(),
    /// #   )
    /// # );
    /// ```
    ///
    /// # rhai-autodocs:index:1
    #[rhai_fn(name = "check", return_raw)]
    pub fn check_no_params(ncc: NativeCallContext) -> EngineResult<Status> {
        super::spf::check_with_params(ncc, rhai::Map::default())
    }

    #[doc(hidden)]
    #[rhai_fn(name = "check", return_raw)]
    pub fn check_with_params(ncc: NativeCallContext, params: rhai::Map) -> EngineResult<Status> {
        let params = rhai::serde::from_dynamic::<SpfParameters>(&params.into())?;
        let ctx = get_global!(ncc, ctx);
        let srv = get_global!(ncc, srv);
        let query = super::check(&ctx, &srv)?;
        let msg = get_global!(ncc, msg);

        let (hostname, sender, client_ip) = {
            let ctx = vsl_guard_ok!(ctx.read());

            (
                vsmtp_plugin_vsl::unix::hostname()?,
                vsl_generic_ok!(ctx.reverse_path()).clone(),
                ctx.client_addr().ip().to_string(),
            )
        };

        // TODO: The Received-SPF header field is a trace field
        //       and SHOULD be prepended to the existing header, above the Received: field
        //       It MUST appear above all other Received-SPF fields in the message.
        match params.header {
            // It is RECOMMENDED that SMTP receivers record the result"
            Headers::Spf => Impl::prepend_header(
                &msg,
                SPF_HEADER,
                &super::spf_header(
                    &query,
                    &hostname,
                    sender.as_ref().map_or("null", |sender| sender.full()),
                    &client_ip,
                ),
            ),
            Headers::Auth => Impl::prepend_header(
                &msg,
                AUTH_HEADER,
                &super::auth_header(
                    &query,
                    &hostname,
                    sender.as_ref().map_or("null", |sender| sender.full()),
                    &client_ip,
                ),
            ),
            Headers::Both => {
                Impl::prepend_header(
                    &msg,
                    AUTH_HEADER,
                    &super::auth_header(
                        &query,
                        &hostname,
                        sender.as_ref().map_or("null", |sender| sender.full()),
                        &client_ip,
                    ),
                );
                Impl::prepend_header(
                    &msg,
                    SPF_HEADER,
                    &super::spf_header(
                        &query,
                        &hostname,
                        sender.as_ref().map_or("null", |sender| sender.full()),
                        &client_ip,
                    ),
                );
            }
            Headers::None => {}
        };

        match params.policy {
            Policy::Strict => {
                Ok(match query.result.as_str() {
                    "pass" => state::next(),
                    "temperror" | "permerror" => {
                        state::deny_with_code(&mut crate::api::code::c550_7_24())?
                    }
                    // "softfail" | "fail"
                    _ => state::deny_with_code(&mut crate::api::code::c550_7_23())?,
                })
            }
            Policy::Soft => {
                Ok(match query.result.as_str() {
                    "pass" | "softfail" => state::next(),
                    "temperror" | "permerror" => {
                        state::deny_with_code(&mut crate::api::code::c550_7_24())?
                    }
                    // "fail"
                    _ => state::deny_with_code(&mut crate::api::code::c550_7_23())?,
                })
            }
        }
    }

    /// WARNING: Low level API, use `spf::check` instead if you do not need
    /// to peek inside the spf result data.
    ///
    /// Check spf record following the Sender Policy Framework (RFC 7208).
    /// see <https://datatracker.ietf.org/doc/html/rfc7208>
    ///
    /// # Return
    ///
    /// * `map` - the result of the spf check, contains the `result`, `mechanism` and `problem` keys.
    ///
    /// # Effective smtp stage
    ///
    /// `mail` and onwards.
    ///
    /// # Note
    ///
    /// `spf::check` only checks for the sender's identity, not the `helo` value.
    ///
    /// # Examples
    ///
    /// ```text
    /// #{
    ///     mail: [
    ///        rule "check spf relay" || {
    ///             const spf = spf::check_raw();
    ///
    ///             log("info", `spf results: ${spf.result}, mechanism: ${spf.mechanism}, problem: ${spf.problem}`)
    ///         },
    ///     ]
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:2
    #[rhai_fn(name = "check_raw", return_raw)]
    pub fn check_raw(ncc: NativeCallContext) -> EngineResult<rhai::Map> {
        let ctx = get_global!(ncc, ctx);
        let srv = get_global!(ncc, srv);

        super::check(&ctx, &srv).map(|spf| result_to_map(&spf))
    }
}

/// Inner spf check implementation.
///
/// # Result
/// * SPF records result.
///
/// # Errors
/// * Pre mail from stage.
/// * Invalid identity.
pub fn check(ctx: &Context, srv: &Server) -> EngineResult<vsmtp_auth::spf::Result> {
    let (spf_sender, ip) = {
        let ctx = vsl_guard_ok!(ctx.read());
        let mail_from = ctx.reverse_path().map_err(Into::<RuntimeError>::into)?;

        let spf_sender = match mail_from {
            Some(mail_from) => vsl_generic_ok!(viaspf::Sender::from_address(mail_from.full())),
            None => {
                let client_name = ctx.client_name().map_err(Into::<RuntimeError>::into)?;
                match client_name {
                    ClientName::Domain(domain) => {
                        vsl_generic_ok!(viaspf::Sender::from_domain(&domain.to_string()))
                    }
                    // See https://www.rfc-editor.org/rfc/rfc7208#section-2.3
                    ClientName::Ip4(_) | ClientName::Ip6(_) => {
                        return Ok(vsmtp_auth::spf::Result {
                            result: "fail".to_owned(),
                            details: vsmtp_auth::spf::Details::Problem(
                                "HELO identity is invalid".to_lowercase(),
                            ),
                        })
                    }
                }
            }
        };

        (spf_sender, ctx.client_addr().ip())
    };

    let resolver = srv.resolvers.get_resolver_root();

    let spf_result = block_on!(vsmtp_auth::spf::evaluate(&resolver, ip, &spf_sender));

    vsl_guard_ok!(ctx.write())
        .set_spf(spf_result.clone())
        .map_err(Into::<RuntimeError>::into)?;

    Ok(spf_result)
}

/// create key-value pairs of spf results
/// to inject into the spf or auth headers.
#[must_use]
pub fn key_value_list(
    spf: &vsmtp_auth::spf::Result,
    hostname: &str,
    sender: &str,
    client_ip: &str,
) -> String {
    format!(
        r#"receiver={};
 client-ip={};
 envelope_from={};
 identity=mailfrom;
 {}`
        "#,
        hostname,
        client_ip,
        sender,
        match &spf.details {
            vsmtp_auth::spf::Details::Mechanism(mechanism) => format!("mechanism={mechanism};"),
            vsmtp_auth::spf::Details::Problem(problem) => format!("problem={problem};"),
        },
    )
}

/// Record results in a spf header (RFC 7208-9)
fn spf_header(
    spf: &vsmtp_auth::spf::Result,
    hostname: &str,
    sender: &str,
    client_ip: &str,
) -> String {
    format!(
        "{} {}",
        spf.result,
        key_value_list(spf, hostname, sender, client_ip)
    )
}

/// Record results in the auth header (RFC 7208-9)
fn auth_header(
    spf: &vsmtp_auth::spf::Result,
    hostname: &str,
    sender: &str,
    client_ip: &str,
) -> String {
    format!(
        r#"{}; spf={}
 reason="{}"
 smtp.mailfrom={}"#,
        hostname,
        spf.result,
        key_value_list(spf, hostname, sender, client_ip),
        sender
    )
}

/// Create a rhai map from spf results.
fn result_to_map(spf: &vsmtp_auth::spf::Result) -> rhai::Map {
    rhai::Map::from_iter([
        ("result".into(), rhai::Dynamic::from(spf.result.clone())),
        match &spf.details {
            vsmtp_auth::spf::Details::Mechanism(mechanism) => {
                ("mechanism".into(), mechanism.into())
            }
            vsmtp_auth::spf::Details::Problem(error) => ("problem".into(), error.into()),
        },
    ])
}
