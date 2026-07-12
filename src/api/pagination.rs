use serde::{Deserialize, Serialize};

use crate::error::{RavynError, Result};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub search: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct PageWindow {
    pub offset: u64,
    pub limit: usize,
}

impl PageWindow {
    pub fn from_query(query: &PageQuery) -> Result<Self> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = decode_cursor(query.cursor.as_deref())?;
        if query
            .search
            .as_deref()
            .is_some_and(|value| value.len() > 256)
        {
            return Err(RavynError::Invalid(
                "pagination search may not exceed 256 characters".into(),
            ));
        }
        Ok(Self { offset, limit })
    }

    pub fn database_limit(self) -> usize {
        self.limit.saturating_add(1)
    }

    pub fn offset_usize(self) -> Result<usize> {
        usize::try_from(self.offset)
            .map_err(|_| RavynError::Invalid("pagination cursor is too large".into()))
    }
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn from_extra_item(mut items: Vec<T>, window: PageWindow) -> Self {
        let has_more = items.len() > window.limit;
        items.truncate(window.limit);
        let next_cursor = has_more.then(|| encode_cursor(window.offset + window.limit as u64));
        Self { items, next_cursor }
    }
}

fn encode_cursor(offset: u64) -> String {
    hex::encode(offset.to_be_bytes())
}

fn decode_cursor(cursor: Option<&str>) -> Result<u64> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let bytes =
        hex::decode(cursor).map_err(|_| RavynError::Invalid("invalid pagination cursor".into()))?;
    let bytes: [u8; 8] = bytes
        .try_into()
        .map_err(|_| RavynError::Invalid("invalid pagination cursor".into()))?;
    Ok(u64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip_is_opaque_and_stable() {
        let encoded = encode_cursor(42);
        assert_eq!(decode_cursor(Some(&encoded)).unwrap(), 42);
        assert!(decode_cursor(Some("not-a-cursor")).is_err());
    }

    #[test]
    fn page_consumes_the_extra_item() {
        let page = Page::from_extra_item(
            vec![1, 2, 3],
            PageWindow {
                offset: 0,
                limit: 2,
            },
        );
        assert_eq!(page.items, vec![1, 2]);
        assert!(page.next_cursor.is_some());
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(500))]

        #[test]
        fn cursors_round_trip_for_any_offset(offset in proptest::num::u64::ANY) {
            use proptest::prelude::prop_assert_eq;
            prop_assert_eq!(decode_cursor(Some(&encode_cursor(offset))).unwrap(), offset);
        }

        #[test]
        fn arbitrary_cursor_text_never_panics(cursor in ".{0,64}") {
            let _ = decode_cursor(Some(&cursor));
        }

        /// Following next_cursor from the start visits every item exactly once,
        /// in order, with no overlap between pages.
        #[test]
        fn windowed_pages_cover_every_item_exactly_once(
            total in 0_usize..500,
            limit in 1_usize..=200,
        ) {
            use proptest::prelude::prop_assert_eq;
            let data: Vec<usize> = (0..total).collect();
            let mut seen = Vec::new();
            let mut cursor: Option<String> = None;
            loop {
                let window = PageWindow::from_query(&PageQuery {
                    cursor: cursor.clone(),
                    limit: Some(limit),
                    search: None,
                })
                .unwrap();
                let items: Vec<usize> = data
                    .iter()
                    .skip(window.offset_usize().unwrap())
                    .take(window.database_limit())
                    .copied()
                    .collect();
                let page = Page::from_extra_item(items, window);
                seen.extend(page.items.iter().copied());
                match page.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => break,
                }
            }
            prop_assert_eq!(seen, data);
        }
    }
}
