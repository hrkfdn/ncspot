//! Spotify's playlist v2 service (the rootlist and playlist contents) over the librespot
//! session.

use http::Method;
use librespot_core::{Session, SpotifyId};
use librespot_protocol::playlist4_external::{ListAttributes, MetaItem, SelectedListContent};
use log::debug;
use protobuf::Message;

use super::error::{InternalError, classify};

/// Rootlist entries per page request.
pub const ROOTLIST_PAGE: usize = 120;
/// Most pages read for the rootlist; past it the listing is incomplete.
pub const MAX_ROOTLIST_PAGES: usize = 50;

const UNNAMED: &str = "Unnamed playlist";

/// One playlist of the rootlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootPlaylist {
    /// Base62 id.
    pub id: String,
    /// "Unnamed playlist" when the service sent no name.
    pub name: String,
    /// Standard base64 of the playlist revision.
    pub snapshot_id: String,
    pub length: usize,
    pub owner_id: String,
    pub collaborative: bool,
    pub cover_url: Option<String>,
    /// Per-playlist status from the service: 200, 403 and so on.
    pub status: i32,
}

/// One entry of the rootlist, in the order the service lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootEntry {
    Playlist(RootPlaylist),
    FolderStart { name: String },
    FolderEnd,
}

/// The whole rootlist: playlists and folder markers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rootlist {
    pub entries: Vec<RootEntry>,
    /// Number of raw items the service reports (folder markers included).
    pub length: usize,
    /// Raw items consumed over all absorbed pages, skipped ones included.
    pub read: usize,
    /// Pages absorbed.
    pub pages: usize,
    /// The rootlist revision, from the first page.
    pub revision: Vec<u8>,
    /// Raw positions (ascending) of the items left out of `entries`, so an entry's rootlist
    /// index can be recovered.
    pub skipped: Vec<usize>,
}

/// A playlist's (or the rootlist's) head without its items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// Empty when the reply carried none (a write must never be based on it).
    pub revision: Vec<u8>,
    /// base64 (standard) of `revision`, the form `Playlist.snapshot_id` stores.
    pub snapshot_id: String,
    /// None when the reply carried none (a missing length is not 0).
    pub length: Option<usize>,
    pub name: Option<String>,
}

/// The rootlist head: each field only when the reply carried it (a missing length is not 0).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RootHead {
    /// Non-empty when present.
    pub revision: Option<Vec<u8>>,
    pub length: Option<usize>,
}

/// One item of a playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistItem {
    pub uri: String,
    /// Unix milliseconds, from the item's `timestamp` attribute.
    pub added_at: Option<i64>,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding. The `base64` crate is only built with `jukebox-graphics`.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(BASE64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode a folder name: `+` is a space and `%XX` is a byte. Malformed escapes stay as typed.
pub(crate) fn decode_name(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => match raw
                .get(i + 1..i + 3)
                .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                Some(b) => {
                    out.push(b);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The last picture size with a URL, else the CDN URL of the picture id.
fn cover_url(attrs: &ListAttributes) -> Option<String> {
    let sized = attrs
        .picture_size
        .iter()
        .rev()
        .filter_map(|p| p.url.as_deref())
        .find(|url| !url.is_empty());
    if let Some(url) = sized {
        return Some(url.to_string());
    }
    let picture = attrs.picture.as_deref().filter(|p| !p.is_empty())?;
    let hex: String = picture.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("https://i.scdn.co/image/{hex}"))
}

fn root_playlist(id: &str, meta: &MetaItem) -> RootPlaylist {
    let attrs = meta.attributes.as_ref();
    RootPlaylist {
        id: id.to_string(),
        name: attrs
            .and_then(|a| a.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| UNNAMED.to_string()),
        snapshot_id: base64_encode(meta.revision.as_deref().unwrap_or_default()),
        length: usize::try_from(meta.length.unwrap_or(0)).unwrap_or(0),
        owner_id: meta.owner_username.clone().unwrap_or_default(),
        collaborative: attrs.and_then(|a| a.collaborative).unwrap_or(false),
        cover_url: attrs.and_then(cover_url),
        status: meta.status_code.unwrap_or(200),
    }
}

/// Add one rootlist page to `list`. Returns the `from` of the next page, or None when the
/// listing is complete or the page cap is reached. A first page without a length or without
/// contents (an empty reply parses as one) is [InternalError::Transient]: taken as a complete
/// empty listing it would prune every stored playlist.
pub fn absorb_rootlist(
    list: &mut Rootlist,
    page: SelectedListContent,
) -> Result<Option<usize>, InternalError> {
    if list.pages == 0 && (page.length.is_none() || page.contents.is_none()) {
        return Err(InternalError::Transient(
            "rootlist reply without length or contents".into(),
        ));
    }
    let total = usize::try_from(page.length.unwrap_or(0)).unwrap_or(0);
    list.length = total;
    if list.pages == 0 {
        list.revision = page.revision.clone().unwrap_or_default();
    }
    let Some(contents) = page.contents.into_option() else {
        return Ok(None);
    };
    let count = contents.items.len();
    let start = list.read;
    list.read += count;
    list.pages += 1;
    let no_meta = MetaItem::default();
    for (i, item) in contents.items.iter().enumerate() {
        let uri = item.uri.as_deref().unwrap_or_default();
        let meta = contents.meta_items.get(i).unwrap_or(&no_meta);
        if let Some(id) = uri.strip_prefix("spotify:playlist:") {
            list.entries
                .push(RootEntry::Playlist(root_playlist(id, meta)));
        } else if let Some(rest) = uri.strip_prefix("spotify:start-group:") {
            let name = rest.split_once(':').map_or("", |(_, name)| name);
            list.entries.push(RootEntry::FolderStart {
                name: decode_name(name),
            });
        } else if uri.starts_with("spotify:end-group:") {
            list.entries.push(RootEntry::FolderEnd);
        } else {
            list.skipped.push(start + i);
        }
    }
    Ok((count > 0 && list.read < total && list.pages < MAX_ROOTLIST_PAGES).then_some(list.read))
}

/// The items of a playlist page, in order.
pub fn parse_contents(content: SelectedListContent) -> Vec<PlaylistItem> {
    let Some(contents) = content.contents.into_option() else {
        return Vec::new();
    };
    contents
        .items
        .into_iter()
        .map(|item| PlaylistItem {
            added_at: item.attributes.as_ref().and_then(|a| a.timestamp),
            uri: item.uri.unwrap_or_default(),
        })
        .collect()
}

/// The head fields of a reply: revision, length and name; a missing revision is empty, a
/// missing length stays missing.
pub fn parse_head(content: &SelectedListContent) -> Head {
    let revision = content.revision.clone().unwrap_or_default();
    Head {
        snapshot_id: base64_encode(&revision),
        revision,
        length: content.length.and_then(|n| usize::try_from(n).ok()),
        name: content.attributes.as_ref().and_then(|a| a.name.clone()),
    }
}

/// The rootlist head fields of a reply, without defaults for missing ones.
pub fn parse_root_head(content: &SelectedListContent) -> RootHead {
    RootHead {
        revision: content.revision.clone().filter(|r| !r.is_empty()),
        length: content.length.and_then(|n| usize::try_from(n).ok()),
    }
}

fn parse(bytes: &[u8]) -> Result<SelectedListContent, InternalError> {
    SelectedListContent::parse_from_bytes(bytes)
        .map_err(|e| InternalError::ProtocolChanged(format!("playlist parse: {e}")))
}

/// The user's rootlist, page by page.
pub async fn rootlist(session: Session) -> Result<Rootlist, InternalError> {
    debug!("rootlist");
    let mut list = Rootlist::default();
    let mut from = 0;
    loop {
        let bytes = session
            .spclient()
            .get_rootlist(from, Some(ROOTLIST_PAGE))
            .await
            .map_err(|e| classify(&e))?;
        match absorb_rootlist(&mut list, parse(&bytes)?)? {
            Some(next) => from = next,
            None => return Ok(list),
        }
    }
}

/// The head of a full read with its items: the revision, length and name are those of the
/// items read, which a later head read may not be.
pub fn parse_listing(content: SelectedListContent) -> (Head, Vec<PlaylistItem>) {
    let head = parse_head(&content);
    (head, parse_contents(content))
}

/// The items of one playlist with the head they were read at. An id that does not parse does
/// not exist.
pub async fn contents(
    session: Session,
    id: String,
) -> Result<(Head, Vec<PlaylistItem>), InternalError> {
    debug!("playlist contents");
    let id = SpotifyId::from_base62(&id).map_err(|_| InternalError::NotFound)?;
    let bytes = session
        .spclient()
        .get_playlist(&id)
        .await
        .map_err(|e| classify(&e))?;
    Ok(parse_listing(parse(&bytes)?))
}

/// The head of one playlist (`?length=0`: no items). An id that does not parse does not exist.
pub async fn head(session: Session, id: String) -> Result<Head, InternalError> {
    debug!("playlist head");
    let id = SpotifyId::from_base62(&id).map_err(|_| InternalError::NotFound)?;
    let endpoint = format!("/playlist/v2/playlist/{}?length=0", id.to_base62());
    let bytes = session
        .spclient()
        .request(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|e| classify(&e))?;
    Ok(parse_head(&parse(&bytes)?))
}

/// Items `from..from + length` of one playlist with the head they were read at (the revision
/// and the whole playlist's length, not the count read). Past the end the items are empty, not
/// an error. An id that does not parse does not exist.
pub async fn range(
    session: Session,
    id: String,
    from: usize,
    length: usize,
) -> Result<(Head, Vec<PlaylistItem>), InternalError> {
    debug!("playlist range");
    let id = SpotifyId::from_base62(&id).map_err(|_| InternalError::NotFound)?;
    let endpoint = format!(
        "/playlist/v2/playlist/{}?from={from}&length={length}",
        id.to_base62()
    );
    let bytes = session
        .spclient()
        .request(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|e| classify(&e))?;
    Ok(parse_listing(parse(&bytes)?))
}

/// The rootlist revision and length, without entries.
pub async fn rootlist_head(session: Session) -> Result<RootHead, InternalError> {
    debug!("rootlist head");
    let bytes = session
        .spclient()
        .get_rootlist(0, Some(0))
        .await
        .map_err(|e| classify(&e))?;
    Ok(parse_root_head(&parse(&bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use librespot_protocol::playlist4_external::{
        Item, ListAttributes, ListItems, MetaItem, PictureSize, SelectedListContent,
    };

    #[test]
    fn a_range_read_reports_the_whole_length_and_its_items() {
        let content = SelectedListContent {
            revision: Some(vec![7]),
            length: Some(3),
            contents: protobuf::MessageField::some(ListItems {
                pos: Some(1),
                items: vec![item("spotify:track:c")],
                ..Default::default()
            }),
            ..Default::default()
        };
        let (head, items) = parse_listing(content);
        assert_eq!(
            head.length,
            Some(3),
            "the playlist length, not the count read"
        );
        assert_eq!(head.revision, vec![7]);
        let uris: Vec<&str> = items.iter().map(|i| i.uri.as_str()).collect();
        assert_eq!(uris, ["spotify:track:c"]);
    }

    fn item(uri: &str) -> Item {
        Item {
            uri: Some(uri.into()),
            ..Default::default()
        }
    }

    fn meta(name: Option<&str>, rev: &[u8], status: i32) -> MetaItem {
        MetaItem {
            revision: Some(rev.to_vec()),
            length: Some(3),
            owner_username: Some("o".into()),
            status_code: Some(status),
            attributes: protobuf::MessageField::some(ListAttributes {
                name: name.map(Into::into),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn page(length: i32, items: Vec<Item>, metas: Vec<MetaItem>) -> SelectedListContent {
        SelectedListContent {
            length: Some(length),
            contents: protobuf::MessageField::some(ListItems {
                pos: Some(0),
                truncated: Some(false),
                items,
                meta_items: metas,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn rootlist_parses_playlists_folders_and_names() {
        let mut list = Rootlist::default();
        let next = absorb_rootlist(
            &mut list,
            page(
                5,
                vec![
                    item("spotify:start-group:f1:My+Folder%21"),
                    item("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
                    item("spotify:playlist:0000000000000000000002"),
                    item("spotify:end-group:f1"),
                    item("spotify:track:4uLU6hMCjMI75M1A2tKUQC"),
                ],
                vec![
                    MetaItem::default(),
                    meta(Some("P1"), b"rev1", 200),
                    meta(None, b"rev2", 403),
                    MetaItem::default(),
                    MetaItem::default(),
                ],
            ),
        );
        assert_eq!(next, Ok(None));
        assert_eq!(list.entries.len(), 4, "the track entry is skipped");
        assert_eq!(
            list.entries[0],
            RootEntry::FolderStart {
                name: "My Folder!".into()
            }
        );
        let RootEntry::Playlist(p1) = &list.entries[1] else {
            panic!("playlist")
        };
        assert_eq!(p1.name, "P1");
        assert_eq!(p1.snapshot_id, "cmV2MQ==");
        assert_eq!(p1.owner_id, "o");
        assert_eq!(p1.length, 3);
        let RootEntry::Playlist(p2) = &list.entries[2] else {
            panic!("playlist")
        };
        assert_eq!(p2.name, "Unnamed playlist");
        assert_eq!(p2.status, 403);
        assert_eq!(list.entries[3], RootEntry::FolderEnd);
    }

    #[test]
    fn rootlist_pages_until_length() {
        let pl = |n: u8| item(&format!("spotify:playlist:00000000000000000000{n:02}"));
        let mut list = Rootlist::default();
        let first = SelectedListContent {
            revision: Some(b"root".to_vec()),
            ..page(
                3,
                vec![pl(1), pl(2)],
                vec![meta(Some("a"), b"r", 200), meta(Some("b"), b"r", 200)],
            )
        };
        assert_eq!(absorb_rootlist(&mut list, first), Ok(Some(2)));
        let second = SelectedListContent {
            revision: Some(b"later".to_vec()),
            ..page(3, vec![pl(3)], vec![meta(Some("c"), b"r", 200)])
        };
        assert_eq!(absorb_rootlist(&mut list, second), Ok(None));
        assert_eq!(list.entries.len(), 3);
        assert_eq!(list.length, 3);
        assert_eq!(
            list.revision,
            b"root".to_vec(),
            "the revision of the first page"
        );
    }

    #[test]
    fn an_empty_first_rootlist_reply_is_transient() {
        let mut list = Rootlist::default();
        assert!(matches!(
            absorb_rootlist(&mut list, SelectedListContent::default()),
            Err(InternalError::Transient(_))
        ));
        let no_length = SelectedListContent {
            length: None,
            ..page(0, vec![], vec![])
        };
        assert!(matches!(
            absorb_rootlist(&mut list, no_length),
            Err(InternalError::Transient(_))
        ));
        let no_contents = SelectedListContent {
            length: Some(2),
            ..Default::default()
        };
        assert!(matches!(
            absorb_rootlist(&mut list, no_contents),
            Err(InternalError::Transient(_))
        ));
        assert_eq!(list.pages, 0, "nothing was absorbed");
    }

    #[test]
    fn an_empty_rootlist_with_a_length_is_complete() {
        let mut list = Rootlist::default();
        assert_eq!(
            absorb_rootlist(&mut list, page(0, vec![], vec![])),
            Ok(None)
        );
        assert_eq!((list.length, list.read, list.pages), (0, 0, 1));
    }

    #[test]
    fn contents_keep_order_and_timestamps() {
        use librespot_protocol::playlist4_external::ItemAttributes;
        let with_ts = |uri: &str, ts: i64| Item {
            uri: Some(uri.into()),
            attributes: protobuf::MessageField::some(ItemAttributes {
                timestamp: Some(ts),
                ..Default::default()
            }),
            ..Default::default()
        };
        let content = page(
            3,
            vec![
                with_ts("spotify:track:a", 5),
                item("spotify:track:b"),
                with_ts("spotify:episode:c", 7),
            ],
            vec![],
        );
        let items = parse_contents(content);
        assert_eq!(
            items,
            vec![
                PlaylistItem {
                    uri: "spotify:track:a".into(),
                    added_at: Some(5)
                },
                PlaylistItem {
                    uri: "spotify:track:b".into(),
                    added_at: None
                },
                PlaylistItem {
                    uri: "spotify:episode:c".into(),
                    added_at: Some(7)
                },
            ]
        );
    }

    #[test]
    fn a_full_read_carries_the_revision_of_its_items() {
        let content = SelectedListContent {
            revision: Some(b"read".to_vec()),
            ..page(
                2,
                vec![item("spotify:track:a"), item("spotify:track:b")],
                vec![],
            )
        };
        let (head, items) = parse_listing(content);
        assert_eq!(head.revision, b"read".to_vec());
        assert_eq!(head.snapshot_id, base64_encode(b"read"));
        assert_eq!(head.length, Some(2));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn cover_prefers_picture_size_then_picture_id() {
        let mut attrs = ListAttributes {
            picture: Some(vec![0xab, 0x01]),
            picture_size: vec![
                PictureSize {
                    target_name: Some("small".into()),
                    url: Some("https://x/s".into()),
                    ..Default::default()
                },
                PictureSize {
                    target_name: Some("large".into()),
                    url: Some("https://x/l".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(cover_url(&attrs).as_deref(), Some("https://x/l"));
        attrs.picture_size.clear();
        assert_eq!(
            cover_url(&attrs).as_deref(),
            Some("https://i.scdn.co/image/ab01")
        );
        attrs.picture = None;
        assert_eq!(cover_url(&attrs), None);
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(&[0xfb, 0xff, 0xbf]), "+/+/");
    }

    #[test]
    fn folder_names_decode_percent_and_plus() {
        assert_eq!(decode_name("My+Folder%21"), "My Folder!");
        assert_eq!(decode_name("100%"), "100%");
        assert_eq!(decode_name("a%zzb"), "a%zzb");
        assert_eq!(decode_name("%+1"), "% 1", "a sign is not a hex digit");
        assert_eq!(decode_name("%C3%A9"), "\u{e9}");
    }

    #[test]
    fn skipped_items_count_as_read_and_do_not_duplicate() {
        let pl = |n: u8| item(&format!("spotify:playlist:00000000000000000000{n:02}"));
        let mut list = Rootlist::default();
        let first = page(
            4,
            vec![pl(1), item("spotify:track:x"), pl(2)],
            vec![
                meta(Some("a"), b"r", 200),
                MetaItem::default(),
                meta(Some("b"), b"r", 200),
            ],
        );
        assert_eq!(absorb_rootlist(&mut list, first), Ok(Some(3)));
        assert_eq!(list.entries.len(), 2);
        let second = page(4, vec![pl(3)], vec![meta(Some("c"), b"r", 200)]);
        assert_eq!(absorb_rootlist(&mut list, second), Ok(None));
        assert_eq!(list.entries.len(), 3);
        assert_eq!((list.read, list.pages), (4, 2));
        assert_eq!(list.skipped, [1], "the raw position of the skipped item");
    }

    #[test]
    fn head_reads_revision_length_and_name() {
        let content = SelectedListContent {
            revision: Some(b"rev1".to_vec()),
            length: Some(5272),
            attributes: protobuf::MessageField::some(ListAttributes {
                name: Some("ncspot-test-1".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let head = parse_head(&content);
        assert_eq!(head.revision, b"rev1".to_vec());
        assert_eq!(head.snapshot_id, "cmV2MQ==");
        assert_eq!(head.length, Some(5272));
        assert_eq!(head.name.as_deref(), Some("ncspot-test-1"));
    }

    #[test]
    fn root_head_keeps_missing_fields_missing() {
        assert_eq!(
            parse_root_head(&SelectedListContent::default()),
            RootHead::default()
        );
        let empty_revision = SelectedListContent {
            revision: Some(Vec::new()),
            length: Some(0),
            ..Default::default()
        };
        assert_eq!(
            parse_root_head(&empty_revision),
            RootHead {
                revision: None,
                length: Some(0)
            }
        );
        let full = SelectedListContent {
            revision: Some(b"r".to_vec()),
            length: Some(7),
            ..Default::default()
        };
        assert_eq!(
            parse_root_head(&full),
            RootHead {
                revision: Some(b"r".to_vec()),
                length: Some(7)
            }
        );
    }

    #[test]
    fn head_without_fields_is_empty() {
        let head = parse_head(&SelectedListContent::default());
        assert_eq!(
            (head.length, head.name, head.snapshot_id.as_str()),
            (None, None, "")
        );
        assert!(head.revision.is_empty());
    }
}
