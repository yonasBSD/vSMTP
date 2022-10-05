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
use crate::{FilesystemQueueManagerExt, QueueID};
use anyhow::Context;
use vsmtp_config::Config;

///
// TODO: handle canonicalization of path (& chown)
pub struct QueueManager {
    config: std::sync::Arc<Config>,
}

impl std::fmt::Debug for QueueManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueManager").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl FilesystemQueueManagerExt for QueueManager {
    fn init(config: std::sync::Arc<Config>) -> anyhow::Result<std::sync::Arc<Self>> {
        <QueueID as strum::IntoEnumIterator>::iter()
            .map(|q| {
                let dir = Self::get_root_folder(&config, &q).join(q.to_string());
                std::fs::create_dir_all(&dir).with_context(|| {
                    format!("could not create `{q}` directory at `{}`", dir.display())
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(std::sync::Arc::new(Self { config }))
    }

    fn get_config(&self) -> &Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use vsmtp_test::config::local_test;

    #[test]
    fn debug() {
        assert_eq!(
            "QueueManager { .. }",
            format!(
                "{:?}",
                <super::QueueManager as crate::GenericQueueManager>::init(std::sync::Arc::new(
                    local_test()
                ))
                .unwrap()
            )
        );
    }
}
