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

/// A domain name.
pub type Domain = trust_dns_resolver::Name;

/// An iterator over the domain name.
///
/// # Example
///
/// ```
/// let domain = "www.john.doe.example.com".parse::<vsmtp_common::Domain>().unwrap();
///
/// let domain_str = domain.to_string();
/// let mut domain_part = vsmtp_common::domain_iter(&domain_str);
/// assert_eq!(domain_part.next().unwrap(), "www.john.doe.example.com");
/// assert_eq!(domain_part.next().unwrap(), "john.doe.example.com");
/// assert_eq!(domain_part.next().unwrap(), "doe.example.com");
/// assert_eq!(domain_part.next().unwrap(), "example.com");
/// assert_eq!(domain_part.next().unwrap(), "com");
/// assert_eq!(domain_part.next(), None);
/// ```
#[must_use]
#[inline]
#[allow(clippy::module_name_repetitions)]
pub fn domain_iter(domain: &str) -> IterDomain<'_> {
    IterDomain::iter(domain)
}

#[allow(clippy::module_name_repetitions)]
pub struct IterDomain<'item>(Option<&'item str>);

impl<'item> IterDomain<'item> {
    /// Create an iterator over the given domain.
    #[must_use]
    pub const fn iter(domain: &'item str) -> Self {
        Self(Some(domain))
    }
}

impl<'item> Iterator for IterDomain<'item> {
    type Item = &'item str;

    fn next(&mut self) -> Option<Self::Item> {
        let out = self.0;
        self.0 = self.0.and_then(|s| s.split_once('.')).map(|(_, rest)| rest);
        out
    }
}
