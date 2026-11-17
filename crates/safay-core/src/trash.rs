//! Recently Deleted. Deleting an account marks it with the time ([`Account::deleted_at`]); it keeps its place, its
//! secret and its categories, so recovering it puts it back as it was. After [`RETENTION`] it is removed for good, as it
//! is at once when the user deletes it from Recently Deleted or empties it.
//!
//! A deleted account is in the vault (and so in backups, encrypted) but is not shown, searched, copied, exported or
//! compared for duplicates; sync merges keep its deletion ([`crate::merge`]).

use std::time::{Duration, SystemTime};

use uuid::Uuid;
use zeroize::Zeroize;

use crate::category;
use crate::model::Account;
use crate::vault::VaultContents;

/// How long a deleted account waits before it is removed for good.
pub const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

const DAY: u64 = 24 * 60 * 60;

/// Whole days left before an account deleted at `deleted_at` is removed (rounded up; 0 once due). A clock set back
/// counts from now, never past the full retention.
pub fn days_left(deleted_at: SystemTime, now: SystemTime) -> u32 {
    let age = now.duration_since(deleted_at).unwrap_or_default();
    let left = RETENTION.saturating_sub(age).as_secs();
    left.div_ceil(DAY) as u32
}

/// Whether an account deleted at `deleted_at` is due for removal.
pub fn is_expired(deleted_at: SystemTime, now: SystemTime) -> bool {
    now.duration_since(deleted_at).is_ok_and(|age| age >= RETENTION)
}

/// The deleted accounts due for removal at `now`.
pub fn expired(accounts: &[Account], now: SystemTime) -> Vec<Uuid> {
    accounts.iter().filter(|a| a.deleted_at.is_some_and(|at| is_expired(at, now))).map(|a| a.id).collect()
}

/// `contents` without the accounts `ids`, their secrets wiped and their category memberships gone.
pub fn removing(contents: &VaultContents, ids: &[Uuid]) -> VaultContents {
    let mut next = contents.clone();
    for account in next.accounts.iter_mut().filter(|a| ids.contains(&a.id)) {
        account.secret.zeroize();
    }
    next.accounts.retain(|a| !ids.contains(&a.id));
    next.categories = category::removing_accounts(&next.categories, ids);
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::Category;
    use crate::model::TotpAlgorithm;
    use std::time::UNIX_EPOCH;

    fn at(days: f64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs_f64(1_000_000_000.0 + days * DAY as f64)
    }
    fn account(n: u128, deleted_at: Option<SystemTime>) -> Account {
        Account {
            id: Uuid::from_u128(n),
            issuer: format!("Service {n}"),
            username: String::new(),
            secret: "JBSWY3DPEHPK3PXP".into(),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            created_at: at(0.0),
            added_by: "manual".into(),
            website: None,
            icon: None,
            deleted_at,
        }
    }

    #[test]
    fn deleted_accounts_wait_thirty_days() {
        let deleted = at(0.0);
        assert_eq!(days_left(deleted, at(0.0)), 30);
        assert_eq!(days_left(deleted, at(0.5)), 30);
        assert_eq!(days_left(deleted, at(29.1)), 1);
        assert_eq!(days_left(deleted, at(30.0)), 0);
        // A clock set back never adds days.
        assert_eq!(days_left(deleted, at(-5.0)), 30);
        assert!(!is_expired(deleted, at(29.9)));
        assert!(is_expired(deleted, at(30.0)));
        assert!(!is_expired(deleted, at(-1.0)));
    }

    #[test]
    fn only_due_accounts_expire_and_removal_clears_their_categories() {
        let contents = VaultContents {
            accounts: vec![account(1, None), account(2, Some(at(0.0))), account(3, Some(at(20.0)))],
            categories: vec![Category {
                id: Uuid::from_u128(9),
                name: "Work".into(),
                account_ids: vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            }],
            pinned_ids: Vec::new(),
        };
        let due = expired(&contents.accounts, at(31.0));
        assert_eq!(due, vec![Uuid::from_u128(2)]);
        let next = removing(&contents, &due);
        assert_eq!(next.accounts.iter().map(|a| a.id.as_u128()).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(next.categories[0].account_ids, vec![Uuid::from_u128(1)]);
    }
}
