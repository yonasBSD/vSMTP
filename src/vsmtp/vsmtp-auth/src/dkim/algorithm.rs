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

/// Hash & sign algorithm exposed in a `DKIM-Signature` header. Used by the
/// expose the algorithm used to verify the message.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, strum::EnumString, strum::Display)]
pub enum SigningAlgorithm {
    /// The SHA-1 hash function should be considered cryptographically broken and unsuitable
    /// for further use in any security critical capacity.
    ///
    /// See the implementation <https://docs.rs/sha1>
    #[cfg_attr(docsrs, doc(cfg(feature = "historic")))]
    #[cfg(feature = "historic")]
    #[strum(serialize = "rsa-sha1")]
    RsaSha1,
    /// See the implementation <https://docs.rs/sha2>
    #[strum(serialize = "rsa-sha256")]
    RsaSha256,
    ///
    #[strum(serialize = "ed25519-sha256")]
    Ed25519Sha256,
}

impl SigningAlgorithm {
    pub(super) fn support_any(self, hash_algo: &[HashAlgorithm]) -> bool {
        let supported = self.get_supported_hash_algo();
        hash_algo.iter().any(|a| supported.contains(a))
    }

    pub(super) const fn get_supported_hash_algo(self) -> &'static [HashAlgorithm] {
        match self {
            #[cfg(feature = "historic")]
            SigningAlgorithm::RsaSha1 => &[HashAlgorithm::Sha1],
            #[cfg(feature = "historic")]
            SigningAlgorithm::RsaSha256 => &[HashAlgorithm::Sha256, HashAlgorithm::Sha1],
            _ => &[HashAlgorithm::Sha256],
        }
    }

    pub(super) fn get_preferred_hash_algo(self) -> &'static HashAlgorithm {
        self.get_supported_hash_algo()
            .first()
            .expect("has at least one algorithm")
    }
}

/// Hash algorithms exposed in the `DKIM record`,
/// used to describe the content of the "p=" tag in the record.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, strum::EnumString, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum HashAlgorithm {
    /// The SHA-1 hash function should be considered cryptographically broken and unsuitable
    /// for further use in any security critical capacity.
    ///
    /// See the implementation <https://docs.rs/sha1>
    #[cfg_attr(docsrs, doc(cfg(feature = "historic")))]
    #[cfg(feature = "historic")]
    Sha1,
    /// See the implementation <https://docs.rs/sha2>
    Sha256,
}

impl HashAlgorithm {
    /// Return the hashed `data` using the algorithm.
    #[must_use]
    pub fn hash<T: AsRef<[u8]>>(self, data: T) -> Vec<u8> {
        match self {
            #[cfg(feature = "historic")]
            HashAlgorithm::Sha1 => {
                let mut digest = <sha1::Sha1 as sha1::Digest>::new();
                sha1::Digest::update(&mut digest, data);
                sha1::Digest::finalize(digest).to_vec()
            }
            HashAlgorithm::Sha256 => {
                let mut digest = <sha2::Sha256 as sha2::Digest>::new();
                sha2::Digest::update(&mut digest, data);
                sha2::Digest::finalize(digest).to_vec()
            }
        }
    }
}
