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

//! vSMTP executable

#![doc(html_no_source)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]
//
#![warn(rust_2018_idioms)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
//
#![allow(clippy::multiple_crate_versions)]
//
#![cfg_attr(
    feature = "document-features",
    doc = ::document_features::document_features!()
)]

mod args;

pub use args::{Args, Commands};

// Tokio-tracing systems
// pub mod tracing_subscriber;

#[cfg(debug_assertions)]
macro_rules! get_fmt {
    () => {
        tracing_subscriber::fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_thread_ids(true)
            .with_target(true)
            .with_ansi(false)
    };
}

#[cfg(not(debug_assertions))]
macro_rules! get_fmt {
    () => {
        tracing_subscriber::fmt::layer()
            .compact()
            .with_thread_ids(false)
            .with_target(false)
            .with_ansi(false)
    };
}

macro_rules! file_writer {
    ($filename:expr, $filter:expr) => {{
        use tracing_subscriber::fmt::writer::MakeWriterExt;

        let filename: &std::path::Path = $filename;
        let writer_backend = if let (Some(directory), Some(file_name)) = (
            filename.parent(),
            filename.file_name().and_then(std::ffi::OsStr::to_str),
        ) {
            tracing_appender::rolling::never(directory, file_name)
        } else {
            anyhow::bail!(
                "filepath at '{}' does not have a parent or is not valid",
                filename.display()
            )
        };

        get_fmt!().with_writer(writer_backend.with_filter($filter))
    }};
}

/// Initialize the tracing subsystem.
///
/// # Errors
///
#[allow(clippy::items_after_statements)]
pub fn init_logs(args: &Args, config: &vsmtp_config::Config) -> anyhow::Result<()> {
    const TARGET_VSL_LOG: &str = "vsmtp_rule_engine::api::logging::logging";
    #[allow(unused_imports)]
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

    let subscriber = tracing_subscriber::registry().with({
        let mut e = tracing_subscriber::EnvFilter::default();
        for i in &config.server.logs.level {
            e = e.add_directive(i.clone());
        }
        e
    });

    #[cfg(feature = "tokio_console")]
    let subscriber = subscriber.with(console_subscriber::spawn());

    #[cfg(feature = "telemetry")]
    let subscriber = subscriber.with(
        tracing_opentelemetry::layer().with_tracer(
            opentelemetry_jaeger::new_agent_pipeline()
                .with_service_name("vsmtp")
                .install_simple()?,
        ),
    );

    let subscriber = subscriber
        .with(file_writer!(
            &config.server.logs.filename,
            |metadata| metadata.target() != TARGET_VSL_LOG
        ))
        .with(file_writer!(&config.app.logs.filename, |metadata| metadata
            .target()
            == TARGET_VSL_LOG));

    #[cfg(feature = "journald")]
    let subscriber = {
        let sys_level = config.server.logs.sys_level;
        subscriber.with(tracing_journald::layer()?.with_filter(
            tracing_subscriber::filter::filter_fn(move |i| *i.level() <= sys_level),
        ))
    };

    // because of the recursive generic implementation of `tracing_subscriber::registry`
    // it is cleaner to implement a macro to avoid code duplication
    macro_rules! try_init {
        ($s:expr) => {
            if args.stdout {
                $s.with(get_fmt!().with_writer(std::io::stdout).with_ansi(true))
                    .try_init()
            } else {
                $s.try_init()
            }?
        };
    }

    cfg_if::cfg_if! {
        if #[cfg(feature = "syslog")] {
            use tracing_rfc_5424::transport::{TcpTransport, UdpTransport, UnixSocket};
            use vsmtp_config::field::SyslogSocket;
            let sys_level = config.server.logs.sys_level;

            macro_rules! syslog_writer {
                ($s:expr, $transport:expr) => {
                    $s.with(
                        tracing_rfc_5424::layer::Layer::with_transport($transport).with_filter(
                            tracing_subscriber::filter::filter_fn(move |i| *i.level() <= sys_level),
                        ),
                    )
                };
            }

            match &config.server.logs.syslog {
                SyslogSocket::Udp { server } => {
                    try_init!(syslog_writer!(subscriber, UdpTransport::new(server)?));
                }
                SyslogSocket::Tcp { server } => {
                    try_init!(syslog_writer!(subscriber, TcpTransport::new(server)?));
                }
                SyslogSocket::Unix { path } => {
                    try_init!(syslog_writer!(subscriber, UnixSocket::new(path)?));
                }
            };
        } else {
            try_init!(subscriber);
        }
    }

    #[allow(unused_mut)]
    let mut debug_info = String::new();
    cfg_if::cfg_if! {
        if #[cfg(feature = "journald")] {
            debug_info += "journald=true,";
        }
    }
    cfg_if::cfg_if! {
        if #[cfg(feature = "syslog")] {
            debug_info += "syslog=true,";
        }
    }
    cfg_if::cfg_if! {
        if #[cfg(feature = "tokio_console")] {
            debug_info += "tokio_console=true,";
        }
    }

    tracing::info!(
        server = ?config.server.logs.filename,
        app = ?config.app.logs.filename,
        stdout = args.stdout,
        "vSMTP logs initialized: {}",
        debug_info
    );

    Ok(())
}
