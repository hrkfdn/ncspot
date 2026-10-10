//! Pure logic for cheap library syncs: detect when a newest-first saved list (liked songs,
//! saved albums, saved shows) is unchanged or only has new items at the top, so a sync can
//! skip downloading the rest. Every shortcut is guarded by an exact check and falls back to a
//! full fetch.

use chrono::{DateTime, Utc};
use log::debug;

/// One item of a saved-list page, reduced to what the sync shortcuts need.
#[derive(Clone, Debug)]
pub(crate) struct SavedItem<T> {
    /// Spotify id (bare, not the URI), if the item has one.
    pub id: Option<String>,
    /// When the user saved the item, if known.
    pub added_at: Option<DateTime<Utc>>,
    /// The converted item to store.
    pub value: T,
}

/// One page of a newest-first saved list.
pub(crate) struct SavedPage<T> {
    pub items: Vec<SavedItem<T>>,
    /// Number of items in the whole remote list.
    pub total: usize,
    /// Whether more pages follow.
    pub has_next: bool,
}

/// Result of syncing a saved list.
#[derive(Debug, PartialEq)]
pub(crate) enum Synced<T> {
    /// Only the first `new_count` items are new; the rest are the cached items.
    Prepended { items: Vec<T>, new_count: usize },
    /// The stored items are current; nothing to change.
    Unchanged,
    /// The whole list was downloaded.
    Full(Vec<T>),
    /// A page failed; keep the stored items.
    Failed,
}

/// Whether fetched liked songs are the cache with new likes in front.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Merge {
    /// The first `new_count` fetched items are new likes and the rest matches the cache.
    Complete { new_count: usize },
    /// The newest cached like was not fetched yet; fetch another page.
    NeedMore,
    /// Anything else: download the whole list.
    Mismatch,
}

/// Compare the ids of the liked songs fetched so far (newest first, from offset 0) with the
/// cached ids. The change is a pure prepend only if the newest cached like is found at some
/// index `k`, everything after it matches the cache, and `total == cached.len() + k`. That
/// arithmetic is exact: a removal anywhere, or a re-like (which moves an old item to the
/// top), breaks it.
pub(crate) fn merge_liked(
    cached: &[Option<String>],
    fetched: &[Option<String>],
    total: usize,
) -> Merge {
    let Some(Some(head)) = cached.first() else {
        return Merge::Mismatch;
    };
    let Some(k) = fetched.iter().position(|id| id.as_ref() == Some(head)) else {
        return if fetched.len() < total {
            Merge::NeedMore
        } else {
            Merge::Mismatch
        };
    };
    let overlap_matches = fetched[k..]
        .iter()
        .zip(cached)
        .all(|(fetched, cached)| fetched.is_some() && fetched == cached);
    if overlap_matches && total == cached.len() + k {
        Merge::Complete { new_count: k }
    } else {
        Merge::Mismatch
    }
}

/// Whether a newest-first saved list is unchanged, judged from its first page: same `total`
/// as the store and every first-page id already stored. Compares sets, not positions, because
/// the album store is sorted by name. A new item would be on the first page and a removal
/// lowers `total`; a re-saved item changes neither, and the result equals a full fetch.
pub(crate) fn first_page_unchanged(
    stored: &[Option<String>],
    page_ids: &[Option<String>],
    total: usize,
) -> bool {
    !stored.is_empty()
        && total == stored.len()
        && page_ids
            .iter()
            .all(|id| id.is_some() && stored.contains(id))
}

/// Whether `dates` never increase, that is the list is newest first. Spotify does not
/// document this order, so the shortcuts check it before relying on it.
pub(crate) fn added_at_descending(dates: &[DateTime<Utc>]) -> bool {
    dates.windows(2).all(|pair| pair[0] >= pair[1])
}

/// Whether every fetched item has an `added_at` and they are newest first.
fn ordered<T>(items: &[SavedItem<T>]) -> bool {
    let dates: Option<Vec<DateTime<Utc>>> = items.iter().map(|item| item.added_at).collect();
    dates.is_some_and(|dates| added_at_descending(&dates))
}

fn ids<T>(items: &[SavedItem<T>]) -> Vec<Option<String>> {
    items.iter().map(|item| item.id.clone()).collect()
}

fn values<T>(items: Vec<SavedItem<T>>) -> Vec<T> {
    items.into_iter().map(|item| item.value).collect()
}

/// Fetch the next page at `fetched.len()` and append it. Returns `(total, has_next)`, or
/// `None` if the request failed or returned an empty page while claiming more follow (which
/// would otherwise loop forever).
fn fetch_into<T>(
    fetched: &mut Vec<SavedItem<T>>,
    fetch_page: &mut impl FnMut(usize) -> Option<SavedPage<T>>,
) -> Option<(usize, bool)> {
    let page = fetch_page(fetched.len())?;
    if page.items.is_empty() && page.has_next {
        return None;
    }
    fetched.extend(page.items);
    Some((page.total, page.has_next))
}

/// Sync liked songs. Fetches pages from the top until the cached songs line up behind the new
/// likes, then returns the new likes followed by the cache. Falls back to downloading the
/// whole list (keeping the pages already fetched) when the dates are not newest first or
/// [merge_liked] reports a mismatch.
pub(crate) fn sync_liked<T: Clone>(
    cached: &[T],
    id_of: impl Fn(&T) -> Option<String>,
    mut fetch_page: impl FnMut(usize) -> Option<SavedPage<T>>,
) -> Synced<T> {
    let cached_ids: Vec<Option<String>> = cached.iter().map(&id_of).collect();
    let mut fetched: Vec<SavedItem<T>> = Vec::new();
    let mut shortcut = true;
    loop {
        let Some((total, has_next)) = fetch_into(&mut fetched, &mut fetch_page) else {
            return Synced::Failed;
        };
        if !has_next {
            return Synced::Full(values(fetched));
        }
        if !shortcut {
            continue;
        }
        if !ordered(&fetched) {
            debug!("liked songs are not newest first, fetching all");
            shortcut = false;
            continue;
        }
        match merge_liked(&cached_ids, &ids(&fetched), total) {
            Merge::Complete { new_count } => {
                let mut items: Vec<T> = values(fetched).into_iter().take(new_count).collect();
                items.extend(cached.iter().cloned());
                return Synced::Prepended { items, new_count };
            }
            Merge::NeedMore => {}
            Merge::Mismatch => {
                debug!("liked songs changed beyond new likes, fetching all");
                shortcut = false;
            }
        }
    }
}

/// Sync a saved list whose store is not in remote order (albums, shows). Returns
/// [Synced::Unchanged] after one request when [first_page_unchanged] holds for a newest-first
/// first page; otherwise downloads the whole list, reusing the first page.
pub(crate) fn sync_saved<T>(
    stored_ids: &[Option<String>],
    mut fetch_page: impl FnMut(usize) -> Option<SavedPage<T>>,
) -> Synced<T> {
    let mut fetched: Vec<SavedItem<T>> = Vec::new();
    let Some((total, mut has_next)) = fetch_into(&mut fetched, &mut fetch_page) else {
        return Synced::Failed;
    };
    if ordered(&fetched) && first_page_unchanged(stored_ids, &ids(&fetched), total) {
        return Synced::Unchanged;
    }
    while has_next {
        let Some((_, next)) = fetch_into(&mut fetched, &mut fetch_page) else {
            return Synced::Failed;
        };
        has_next = next;
    }
    Synced::Full(values(fetched))
}

#[cfg(test)]
mod test {
    use super::*;
    use std::cell::Cell;

    const PAGE: usize = 50;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn some(ids: &[&str]) -> Vec<Option<String>> {
        ids.iter().map(|id| Some(id.to_string())).collect()
    }

    /// `count` ids `<prefix>0`, `<prefix>1`, ...
    fn names(prefix: &str, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("{prefix}{i}")).collect()
    }

    /// A remote newest-first list: `added_at` strictly decreasing.
    fn remote(ids: &[String]) -> Vec<SavedItem<String>> {
        ids.iter()
            .enumerate()
            .map(|(i, id)| SavedItem {
                id: Some(id.clone()),
                added_at: Some(at(1_000_000 - i as i64)),
                value: id.clone(),
            })
            .collect()
    }

    /// A page source over `list` that counts its requests in `calls`.
    fn source<'a>(
        list: &'a [SavedItem<String>],
        calls: &'a Cell<usize>,
    ) -> impl FnMut(usize) -> Option<SavedPage<String>> + 'a {
        move |offset| {
            calls.set(calls.get() + 1);
            let end = (offset + PAGE).min(list.len());
            Some(SavedPage {
                items: list[offset..end].to_vec(),
                total: list.len(),
                has_next: end < list.len(),
            })
        }
    }

    // merge_liked

    #[test]
    fn merge_unchanged() {
        let cached = some(&["a", "b", "c"]);
        assert_eq!(
            merge_liked(&cached, &some(&["a", "b", "c"]), 3),
            Merge::Complete { new_count: 0 }
        );
    }

    #[test]
    fn merge_new_likes_on_one_page() {
        let cached = some(&["a", "b", "c"]);
        let fetched = some(&["x", "y", "z", "a", "b", "c"]);
        assert_eq!(
            merge_liked(&cached, &fetched, 6),
            Merge::Complete { new_count: 3 }
        );
    }

    #[test]
    fn merge_needs_more_until_the_old_head_shows_up() {
        let cached = some(&["a", "b"]);
        assert_eq!(merge_liked(&cached, &some(&["x", "y"]), 5), Merge::NeedMore);
        assert_eq!(
            merge_liked(&cached, &some(&["x", "y", "z", "a", "b"]), 5),
            Merge::Complete { new_count: 3 }
        );
    }

    #[test]
    fn merge_removal_deep_in_the_list() {
        let cached = some(&["a", "b", "c", "d"]);
        // "d" was removed beyond the fetched page: total is one lower.
        assert_eq!(merge_liked(&cached, &some(&["a", "b"]), 3), Merge::Mismatch);
    }

    #[test]
    fn merge_re_like_moves_an_item_to_the_top() {
        let cached = some(&["a", "b", "c"]);
        // "c" was unliked and liked again: same total, "c" counted as new.
        assert_eq!(
            merge_liked(&cached, &some(&["c", "a", "b"]), 3),
            Merge::Mismatch
        );
    }

    #[test]
    fn merge_overlap_differs() {
        let cached = some(&["a", "b", "c", "d"]);
        assert_eq!(
            merge_liked(&cached, &some(&["a", "b", "x", "d"]), 4),
            Merge::Mismatch
        );
    }

    #[test]
    fn merge_empty_cache() {
        assert_eq!(merge_liked(&[], &some(&["a"]), 1), Merge::Mismatch);
    }

    #[test]
    fn merge_none_id_in_the_overlap() {
        let cached = vec![Some("a".to_string()), None];
        let fetched = vec![Some("a".to_string()), None];
        assert_eq!(merge_liked(&cached, &fetched, 2), Merge::Mismatch);
    }

    #[test]
    fn merge_old_head_missing_from_the_whole_list() {
        let cached = some(&["a", "b"]);
        assert_eq!(merge_liked(&cached, &some(&["x", "b"]), 2), Merge::Mismatch);
    }

    // first_page_unchanged

    #[test]
    fn first_page_same_set_and_total() {
        let stored = some(&["c", "a", "b"]);
        assert!(first_page_unchanged(&stored, &some(&["a", "b"]), 3));
    }

    #[test]
    fn first_page_with_a_new_item() {
        let stored = some(&["a", "b", "c"]);
        assert!(!first_page_unchanged(&stored, &some(&["x", "a"]), 3));
    }

    #[test]
    fn first_page_total_lower() {
        let stored = some(&["a", "b", "c"]);
        assert!(!first_page_unchanged(&stored, &some(&["a", "b"]), 2));
    }

    #[test]
    fn first_page_re_saved_item() {
        let stored = some(&["a", "b", "c"]);
        assert!(first_page_unchanged(&stored, &some(&["c", "a"]), 3));
    }

    #[test]
    fn first_page_empty_store() {
        assert!(!first_page_unchanged(&[], &[], 0));
    }

    #[test]
    fn first_page_none_id() {
        let stored = some(&["a", "b"]);
        assert!(!first_page_unchanged(&stored, &[None], 2));
    }

    // added_at_descending

    #[test]
    fn descending_with_ties() {
        assert!(added_at_descending(&[at(30), at(20), at(20), at(10)]));
    }

    #[test]
    fn not_descending() {
        assert!(!added_at_descending(&[at(30), at(40), at(10)]));
    }

    #[test]
    fn descending_trivial() {
        assert!(added_at_descending(&[]));
        assert!(added_at_descending(&[at(5)]));
    }

    // sync_liked

    #[test]
    fn liked_unchanged_costs_one_request() {
        let cached = names("c", 120);
        let list = remote(&cached);
        let calls = Cell::new(0);
        let synced = sync_liked(&cached, |v| Some(v.clone()), source(&list, &calls));
        assert_eq!(
            synced,
            Synced::Prepended {
                items: cached.clone(),
                new_count: 0
            }
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn liked_new_likes_on_the_first_page() {
        let cached = names("c", 120);
        let mut ids = names("n", 3);
        ids.extend(cached.iter().cloned());
        let list = remote(&ids);
        let calls = Cell::new(0);
        let synced = sync_liked(&cached, |v| Some(v.clone()), source(&list, &calls));
        assert_eq!(
            synced,
            Synced::Prepended {
                items: ids,
                new_count: 3
            }
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn liked_new_likes_spanning_two_pages() {
        let cached = names("c", 120);
        let mut ids = names("n", 60);
        ids.extend(cached.iter().cloned());
        let list = remote(&ids);
        let calls = Cell::new(0);
        let synced = sync_liked(&cached, |v| Some(v.clone()), source(&list, &calls));
        assert_eq!(
            synced,
            Synced::Prepended {
                items: ids,
                new_count: 60
            }
        );
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn liked_removal_deep_falls_back_to_full() {
        let cached = names("c", 120);
        let ids: Vec<String> = cached.iter().filter(|id| *id != "c100").cloned().collect();
        let list = remote(&ids);
        let calls = Cell::new(0);
        assert_eq!(
            sync_liked(&cached, |v| Some(v.clone()), source(&list, &calls)),
            Synced::Full(ids)
        );
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn liked_re_like_falls_back_to_full() {
        let cached = names("c", 120);
        let mut ids = vec!["c100".to_string()];
        ids.extend(cached.iter().filter(|id| *id != "c100").cloned());
        let list = remote(&ids);
        let calls = Cell::new(0);
        assert_eq!(
            sync_liked(&cached, |v| Some(v.clone()), source(&list, &calls)),
            Synced::Full(ids)
        );
    }

    #[test]
    fn liked_empty_cache_is_a_full_fetch() {
        let ids = names("n", 70);
        let list = remote(&ids);
        let calls = Cell::new(0);
        assert_eq!(
            sync_liked(&[], |v| Some(v.clone()), source(&list, &calls)),
            Synced::Full(ids)
        );
    }

    #[test]
    fn liked_unordered_dates_fall_back_to_full() {
        let cached = names("c", 120);
        let mut list = remote(&cached);
        list[1].added_at = Some(at(2_000_000));
        let calls = Cell::new(0);
        assert_eq!(
            sync_liked(&cached, |v| Some(v.clone()), source(&list, &calls)),
            Synced::Full(cached)
        );
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn liked_failed_page_keeps_the_cache() {
        let cached = names("c", 120);
        let mut ids = names("n", 60);
        ids.extend(cached.iter().cloned());
        let list = remote(&ids);
        let calls = Cell::new(0);
        let mut inner = source(&list, &calls);
        let failing = |offset: usize| if offset == 0 { inner(offset) } else { None };
        assert_eq!(
            sync_liked(&cached, |v| Some(v.clone()), failing),
            Synced::Failed
        );
    }

    #[test]
    fn liked_empty_page_with_more_to_come_is_a_failure() {
        let cached = names("c", 3);
        let empty = |_: usize| {
            Some(SavedPage::<String> {
                items: Vec::new(),
                total: 10,
                has_next: true,
            })
        };
        assert_eq!(
            sync_liked(&cached, |v| Some(v.clone()), empty),
            Synced::Failed
        );
    }

    // sync_saved

    #[test]
    fn saved_unchanged_costs_one_request() {
        let ids = names("a", 120);
        let list = remote(&ids);
        // The store is sorted differently (by name, like the album store).
        let mut stored: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
        stored.reverse();
        let calls = Cell::new(0);
        assert_eq!(
            sync_saved(&stored, source(&list, &calls)),
            Synced::Unchanged
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn saved_new_item_is_a_full_fetch() {
        let old = names("a", 120);
        let stored: Vec<Option<String>> = old.iter().cloned().map(Some).collect();
        let mut ids = vec!["new".to_string()];
        ids.extend(old);
        let list = remote(&ids);
        let calls = Cell::new(0);
        assert_eq!(
            sync_saved(&stored, source(&list, &calls)),
            Synced::Full(ids)
        );
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn saved_removal_is_a_full_fetch() {
        let old = names("a", 120);
        let stored: Vec<Option<String>> = old.iter().cloned().map(Some).collect();
        let ids: Vec<String> = old.into_iter().filter(|id| id != "a100").collect();
        let list = remote(&ids);
        let calls = Cell::new(0);
        assert_eq!(
            sync_saved(&stored, source(&list, &calls)),
            Synced::Full(ids)
        );
    }

    #[test]
    fn saved_unordered_dates_are_a_full_fetch() {
        let ids = names("a", 120);
        let stored: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
        let mut list = remote(&ids);
        list[3].added_at = None;
        let calls = Cell::new(0);
        assert_eq!(
            sync_saved(&stored, source(&list, &calls)),
            Synced::Full(ids)
        );
    }

    #[test]
    fn saved_failed_page_keeps_the_store() {
        let stored = some(&["a"]);
        assert_eq!(sync_saved::<String>(&stored, |_| None), Synced::Failed);
    }
}
