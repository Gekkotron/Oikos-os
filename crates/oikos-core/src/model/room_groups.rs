//! Room groups, as the app's room-group rule: one card for all covers (or all lights) of a room.

use super::{Kind, UNASSIGNED_AREA, UiOverride};

/// A room holds a group only from this many members.
pub const MIN_GROUP_MEMBERS: usize = 2;

/// The kinds that group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GroupKind {
    Cover,
    Light,
}

impl GroupKind {
    /// `None`: this kind never groups.
    pub fn of(kind: Kind) -> Option<GroupKind> {
        match kind {
            Kind::Cover => Some(GroupKind::Cover),
            Kind::Light => Some(GroupKind::Light),
            _ => None,
        }
    }

    /// The key used in `groupNames`, `groupFavorites` and group ids.
    pub fn key(self) -> &'static str {
        match self {
            GroupKind::Cover => "cover",
            GroupKind::Light => "light",
        }
    }

    pub fn from_key(key: &str) -> Option<GroupKind> {
        match key {
            "cover" => Some(GroupKind::Cover),
            "light" => Some(GroupKind::Light),
            _ => None,
        }
    }

    /// The card name when the room has no custom one.
    pub fn default_name(self) -> &'static str {
        match self {
            GroupKind::Cover => "Shutters",
            GroupKind::Light => "Lights",
        }
    }
}

/// One room's group of one kind.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupKey {
    pub area: String,
    pub kind: GroupKind,
}

impl GroupKey {
    /// The group this entity would join, before the 2-member threshold; `None` for other kinds,
    /// the Unassigned area or an entity opted out with `ui.ungrouped`.
    pub fn candidate(area: &str, kind: Kind, ui: &UiOverride) -> Option<GroupKey> {
        if ui.ungrouped || area == UNASSIGNED_AREA {
            return None;
        }
        Some(GroupKey {
            area: area.to_owned(),
            kind: GroupKind::of(kind)?,
        })
    }

    /// `group:<area>:<kind key>`.
    pub fn id(&self) -> String {
        format!("group:{}:{}", self.area, self.kind.key())
    }

    /// Inverse of [`GroupKey::id`]; the area may itself hold colons, so it splits on the last one.
    pub fn parse(id: &str) -> Option<GroupKey> {
        let (area, key) = id.strip_prefix("group:")?.rsplit_once(':')?;
        Some(GroupKey {
            area: area.to_owned(),
            kind: GroupKind::from_key(key)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_keys() {
        assert_eq!(GroupKind::of(Kind::Cover), Some(GroupKind::Cover));
        assert_eq!(GroupKind::of(Kind::Switch), None);
        assert_eq!(GroupKind::from_key("light"), Some(GroupKind::Light));
        assert_eq!(GroupKind::from_key("switch"), None);
        assert_eq!(GroupKind::Cover.default_name(), "Shutters");
        assert_eq!(GroupKind::Light.default_name(), "Lights");
    }

    #[test]
    fn candidate_rules() {
        let ui = UiOverride::default();
        assert!(GroupKey::candidate("living", Kind::Cover, &ui).is_some());
        let out = UiOverride {
            ungrouped: true,
            ..UiOverride::default()
        };
        assert_eq!(GroupKey::candidate("living", Kind::Cover, &out), None);
        assert_eq!(GroupKey::candidate(UNASSIGNED_AREA, Kind::Cover, &ui), None);
        assert_eq!(GroupKey::candidate("living", Kind::Sensor, &ui), None);
    }

    #[test]
    fn id_round_trip() {
        let key = GroupKey {
            area: "bedroom3".to_owned(),
            kind: GroupKind::Light,
        };
        assert_eq!(key.id(), "group:bedroom3:light");
        assert_eq!(GroupKey::parse(&key.id()), Some(key));
        assert_eq!(GroupKey::parse("group:a:b:cover").unwrap().area, "a:b");
        assert_eq!(GroupKey::parse("entity"), None);
        assert_eq!(GroupKey::parse("group:x:switch"), None);
    }
}
