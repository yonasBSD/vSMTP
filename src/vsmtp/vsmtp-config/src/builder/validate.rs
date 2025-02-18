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
use super::{wants::WantsValidate, with::Builder};
use crate::{
    config::field::{
        FieldApp, FieldAppLogs, FieldAppVSL, FieldServer, FieldServerInterfaces, FieldServerLogs,
        FieldServerQueues, FieldServerSMTP, FieldServerSMTPError, FieldServerSMTPTimeoutClient,
        FieldServerSystem, FieldServerSystemThreadPool,
    },
    Config,
};

impl Builder<WantsValidate> {
    ///
    ///
    /// # Errors
    ///
    /// *
    #[must_use]
    pub fn validate(self) -> Config {
        let virtual_entries = self.state;
        let dns = virtual_entries.parent;
        let app_logs = dns.parent;
        let app_vsl = app_logs.parent;
        let app = app_vsl.parent;
        let esmtp = app.parent;
        let smtp_error = esmtp.parent;
        let smtp_opt = smtp_error.parent;
        let srv_tls = smtp_opt.parent;
        let srv_delivery = srv_tls.parent;
        let srv_logs = srv_delivery.parent;
        let srv_inet = srv_logs.parent;
        let srv_syst = srv_inet.parent;
        let srv = srv_syst.parent;
        let path = srv.parent;
        let version = path.parent;

        Config {
            version_requirement: version.version_requirement,
            path: path.path,
            server: FieldServer {
                name: srv.name,
                client_count_max: srv.client_count_max,
                message_size_limit: srv.message_size_limit,
                system: FieldServerSystem {
                    user: srv_syst.user,
                    group: srv_syst.group,
                    group_local: srv_syst.group_local,
                    thread_pool: FieldServerSystemThreadPool {
                        receiver: srv_syst.thread_pool_receiver,
                        processing: srv_syst.thread_pool_processing,
                        delivery: srv_syst.thread_pool_delivery,
                    },
                },
                interfaces: FieldServerInterfaces {
                    addr: srv_inet.addr,
                    addr_submission: srv_inet.addr_submission,
                    addr_submissions: srv_inet.addr_submissions,
                },
                logs: FieldServerLogs {
                    filename: srv_logs.filename,
                    level: srv_logs.level,
                    #[cfg(any(feature = "journald", feature = "syslog"))]
                    sys_level: FieldServerLogs::default_sys_level(),
                    #[cfg(feature = "syslog")]
                    syslog: crate::field::SyslogSocket::default(),
                },
                queues: FieldServerQueues {
                    dirpath: srv_delivery.dirpath,
                    working: srv_delivery.working,
                    delivery: srv_delivery.delivery,
                },
                tls: srv_tls.tls,
                smtp: FieldServerSMTP {
                    rcpt_count_max: smtp_opt.rcpt_count_max,
                    error: FieldServerSMTPError {
                        soft_count: smtp_error.error.soft_count,
                        hard_count: smtp_error.error.hard_count,
                        delay: smtp_error.error.delay,
                    },
                    timeout_client: FieldServerSMTPTimeoutClient {
                        connect: smtp_error.timeout_client.connect,
                        helo: smtp_error.timeout_client.helo,
                        mail_from: smtp_error.timeout_client.mail_from,
                        rcpt_to: smtp_error.timeout_client.rcpt_to,
                        data: smtp_error.timeout_client.data,
                    },
                },
                esmtp: esmtp.esmtp,
                dns: dns.config,
                r#virtual: virtual_entries.r#virtual,
            },
            app: FieldApp {
                dirpath: app.dirpath,
                vsl: FieldAppVSL {
                    domain_dir: app_vsl.domain_dir,
                    filter_path: app_vsl.filter_path,
                },
                logs: FieldAppLogs {
                    filename: app_logs.filename,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Config;

    #[test]
    fn default_build() {
        let _config = Config::builder()
            .with_current_version()
            .without_path()
            .with_server_name("testserver.com".parse::<vsmtp_common::Domain>().unwrap())
            .with_default_system()
            .with_ipv4_localhost()
            .with_default_logs_settings()
            .with_default_delivery()
            .without_tls_support()
            .with_default_smtp_options()
            .with_default_smtp_error_handler()
            .with_default_extensions()
            .with_default_app()
            .with_default_vsl_settings()
            .with_default_app_logs()
            .with_system_dns()
            .without_virtual_entries()
            .validate();
    }
}
