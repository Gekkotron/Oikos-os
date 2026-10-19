//! The push alerts the hub knows: the last parsed history payload, kept in memory. The
//! broker's retained message is the store; this is only its decoded copy.

use std::hash::{DefaultHasher, Hash, Hasher};

use oikos_api::{AlertView, parse_history};

/// What applying a history payload did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The list changed; here it is, for the broadcast.
    Changed(Vec<AlertView>),
    /// The payload decoded to the list already held.
    Unchanged,
    /// The payload is not a JSON array; the list is kept. `first_time` is false when the same
    /// bad payload was seen before, so the caller logs it once.
    Invalid { first_time: bool },
}

#[derive(Debug, Default)]
pub struct AlertStore {
    list: Vec<AlertView>,
    last_bad_payload: Option<u64>,
}

impl AlertStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one history payload. An empty payload (the retained message was cleared)
    /// empties the list.
    pub fn apply(&mut self, payload: &[u8]) -> Applied {
        let next = if payload.is_empty() {
            Vec::new()
        } else {
            match parse_history(payload) {
                Some(list) => list,
                None => {
                    let mut hasher = DefaultHasher::new();
                    payload.hash(&mut hasher);
                    let hash = hasher.finish();
                    let first_time = self.last_bad_payload != Some(hash);
                    self.last_bad_payload = Some(hash);
                    return Applied::Invalid { first_time };
                }
            }
        };
        if next == self.list {
            return Applied::Unchanged;
        }
        self.list = next;
        Applied::Changed(self.list.clone())
    }

    pub fn list(&self) -> &[AlertView] {
        &self.list
    }

    pub fn find(&self, id: &str) -> Option<&AlertView> {
        self.list.iter().find(|a| a.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &[u8] = br#"[{"v":1,"id":"a","title":"A","sent_at":1}]"#;

    #[test]
    fn changed_then_unchanged() {
        let mut store = AlertStore::new();
        assert!(matches!(store.apply(ENTRY), Applied::Changed(list) if list.len() == 1));
        assert_eq!(store.apply(ENTRY), Applied::Unchanged);
        assert_eq!(store.find("a").map(|a| a.title.as_str()), Some("A"));
        assert!(store.find("b").is_none());
    }

    #[test]
    fn empty_payload_clears() {
        let mut store = AlertStore::new();
        store.apply(ENTRY);
        assert_eq!(store.apply(b""), Applied::Changed(vec![]));
        assert!(store.list().is_empty());
        assert_eq!(store.apply(b""), Applied::Unchanged);
    }

    #[test]
    fn invalid_payload_is_reported_once_per_payload() {
        let mut store = AlertStore::new();
        store.apply(ENTRY);
        assert_eq!(store.apply(b"{}"), Applied::Invalid { first_time: true });
        assert_eq!(store.apply(b"{}"), Applied::Invalid { first_time: false });
        assert_eq!(store.apply(b"nope"), Applied::Invalid { first_time: true });
        assert_eq!(store.list().len(), 1);
    }
}
