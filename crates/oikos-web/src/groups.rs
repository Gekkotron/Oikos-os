//! Room groups on Home: which cards a room shows (two or more covers, or two or more lights,
//! fold into one group card) and the text and maths of a group. Pure; the cards live elsewhere.

use std::collections::{BTreeMap, HashMap};

use oikos_api::{AreaView, EntityView};
use oikos_core::model::{
    Availability, Command, CoverState, GroupKey, GroupKind, MIN_GROUP_MEMBERS, StateValue,
};

use crate::api::encode_id;
use crate::cards::cover::is_moving;
use crate::format::is_on;
use crate::screens::home::{FAVORITES, shows_on_home, without_paired_humidity};

/// One card on Home: a single entity, or the group card of a room. A group carries the ids of
/// its members in Home order: under Favorites that can be fewer than the room's group.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HomeItem {
    Entity(String),
    Group { key: GroupKey, members: Vec<String> },
}

/// The entities Home shows, in Home order (the hub's `order` override first, then by name).
fn shown<'a>(entities: impl Iterator<Item = &'a EntityView>) -> Vec<&'a EntityView> {
    let mut shown: Vec<&EntityView> = entities.filter(|e| shows_on_home(e)).collect();
    shown.sort_by_cached_key(|e| (e.ui.order.unwrap_or(i32::MAX), e.name.to_lowercase()));
    shown
}

/// The groups that exist among `shown`, each with its member count.
fn groups_of(shown: &[&EntityView]) -> HashMap<GroupKey, usize> {
    let mut counts = HashMap::new();
    for e in shown {
        if let Some(k) = GroupKey::candidate(&e.area, e.kind, &e.ui) {
            *counts.entry(k).or_insert(0) += 1;
        }
    }
    counts.retain(|_, n| *n >= MIN_GROUP_MEMBERS);
    counts
}

/// What Home shows under `chip`, in display order: filter first, then fold what is left (the
/// app's `HomeUiBuilder` then `RoomGroups.fold`). A room chip shows the whole room; Favorites
/// shows the favourite entities and every member of a favourited group. Each group folds into
/// its first member's slot, then pinned cards, then by room, groups before singles.
pub fn home_items<'a>(
    entities: impl Iterator<Item = &'a EntityView>,
    areas: &[AreaView],
    chip: &str,
) -> Vec<HomeItem> {
    let all = shown(entities);
    let in_room = groups_of(&all);
    let candidates: Vec<&EntityView> = all
        .into_iter()
        .filter(|e| {
            if chip != FAVORITES {
                return e.area == chip;
            }
            let favourited_group = GroupKey::candidate(&e.area, e.kind, &e.ui)
                .is_some_and(|k| in_room.contains_key(&k) && is_favorite(areas, &k));
            e.ui.favorite || favourited_group
        })
        .collect();
    let candidates = without_paired_humidity(candidates);
    let groups = groups_of(&candidates);

    let mut placed: Vec<(i32, usize, HomeItem)> = Vec::new();
    // Where each group sits in `placed`, so later members join it.
    let mut slots: HashMap<GroupKey, usize> = HashMap::new();
    for e in candidates {
        let area_index = areas
            .iter()
            .position(|a| a.id == e.area)
            .unwrap_or(usize::MAX);
        let group = GroupKey::candidate(&e.area, e.kind, &e.ui).filter(|k| groups.contains_key(k));
        match group {
            Some(k) => match slots.get(&k) {
                Some(&slot) => {
                    if let HomeItem::Group { members, .. } = &mut placed[slot].2 {
                        members.push(e.id.clone());
                    }
                }
                None => {
                    slots.insert(k.clone(), placed.len());
                    placed.push((
                        i32::MAX,
                        area_index,
                        HomeItem::Group {
                            key: k,
                            members: vec![e.id.clone()],
                        },
                    ));
                }
            },
            None => placed.push((
                e.ui.order.unwrap_or(i32::MAX),
                area_index,
                HomeItem::Entity(e.id.clone()),
            )),
        }
    }
    placed.sort_by_key(|(order, area, item)| {
        (*order, *area, !matches!(item, HomeItem::Group { .. }))
    });
    placed.into_iter().map(|(_, _, item)| item).collect()
}

/// The members of `key` in Home order; empty when the group does not exist (fewer than two).
pub fn members(entities: &BTreeMap<String, EntityView>, key: &GroupKey) -> Vec<EntityView> {
    let found: Vec<EntityView> = shown(entities.values())
        .into_iter()
        .filter(|e| GroupKey::candidate(&e.area, e.kind, &e.ui).as_ref() == Some(key))
        .cloned()
        .collect();
    if found.len() < MIN_GROUP_MEMBERS {
        return Vec::new();
    }
    found
}

fn area_of<'a>(areas: &'a [AreaView], key: &GroupKey) -> Option<&'a AreaView> {
    areas.iter().find(|a| a.id == key.area)
}

/// The group's name: the room's custom name for the kind, else "Shutters" / "Lights".
pub fn group_name(areas: &[AreaView], key: &GroupKey) -> String {
    area_of(areas, key)
        .and_then(|a| a.group_names.get(key.kind.key()))
        .map(|n| n.trim())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| key.kind.default_name())
        .to_owned()
}

/// Whether the group is a favourite of its room.
pub fn is_favorite(areas: &[AreaView], key: &GroupKey) -> bool {
    area_of(areas, key).is_some_and(|a| a.group_favorites.contains(key.kind.key()))
}

/// How many cards the Favorites chip would show (a folded group counts once).
pub fn favorites_count<'a>(
    entities: impl Iterator<Item = &'a EntityView>,
    areas: &[AreaView],
) -> usize {
    home_items(entities, areas, FAVORITES).len()
}

/// `/group/{area}/{kind}`, the route of a group's sheet.
pub fn group_path(key: &GroupKey) -> String {
    format!("/group/{}/{}", encode_id(&key.area), key.kind.key())
}

/// The group a `/group/:area/:kind` route names. The router hands over decoded params.
pub fn parse_group_path(area: &str, kind: &str) -> Option<GroupKey> {
    Some(GroupKey {
        area: area.to_owned(),
        kind: GroupKind::from_key(kind)?,
    })
}

/// The way a cover group is moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Opening,
    Closing,
}

/// A group's state boiled down for its card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSummary {
    /// Members that are on (lights) or open (covers).
    pub on: usize,
    pub total: usize,
    pub moving: Option<Motion>,
    /// Each member's position for the mini bars, 0..=100.
    pub positions: Vec<u8>,
    /// Any member online.
    pub available: bool,
}

fn motion_of(text: &str) -> Option<Motion> {
    if text.eq_ignore_ascii_case("opening") {
        Some(Motion::Opening)
    } else if text.eq_ignore_ascii_case("closing") {
        Some(Motion::Closing)
    } else {
        None
    }
}

/// How `member` moves: by its own state, else by a folded "moving" child (the child's text gives
/// the direction, opening when it gives none).
fn motion(member: &EntityView, entities: &BTreeMap<String, EntityView>) -> Option<Motion> {
    if let StateValue::Cover(CoverState {
        state: Some(state), ..
    }) = &member.state.value
        && let Some(m) = motion_of(state)
    {
        return Some(m);
    }
    let children: Vec<&EntityView> = entities
        .values()
        .filter(|c| c.part_of.as_deref() == Some(member.id.as_str()))
        .collect();
    if !is_moving(children.iter().copied()) {
        return None;
    }
    let from_text = children.iter().find_map(|c| match &c.state.value {
        StateValue::Text(t) => motion_of(t),
        _ => None,
    });
    Some(from_text.unwrap_or(Motion::Opening))
}

fn position_of(member: &EntityView) -> u8 {
    match &member.state.value {
        StateValue::Cover(CoverState {
            position: Some(p), ..
        }) => (*p).min(100),
        _ => {
            if is_on(&member.state) {
                100
            } else {
                0
            }
        }
    }
}

/// `members` boiled down; `entities` supplies the folded children that say a cover moves.
pub fn summary(members: &[EntityView], entities: &BTreeMap<String, EntityView>) -> GroupSummary {
    let motions: Vec<Motion> = members.iter().filter_map(|m| motion(m, entities)).collect();
    let moving = if motions.contains(&Motion::Opening) {
        Some(Motion::Opening)
    } else {
        motions.first().copied()
    };
    GroupSummary {
        on: members.iter().filter(|m| is_on(&m.state)).count(),
        total: members.len(),
        moving,
        positions: members.iter().map(position_of).collect(),
        available: members.iter().any(is_available),
    }
}

/// A member that can take a command: anything not known to be offline.
fn is_available(e: &EntityView) -> bool {
    e.state.availability != Availability::Offline
}

/// The card text of a group with no member online.
const UNAVAILABLE: &str = "Unavailable";

/// "Unavailable" when no member is online, else "Opening…", "Closing…", "All open", "All closed" or "N open · M closed".
pub fn cover_text(s: &GroupSummary) -> String {
    if !s.available {
        return UNAVAILABLE.into();
    }
    match s.moving {
        Some(Motion::Opening) => "Opening…".into(),
        Some(Motion::Closing) => "Closing…".into(),
        None if s.on == s.total => "All open".into(),
        None if s.on == 0 => "All closed".into(),
        None => format!("{} open · {} closed", s.on, s.total - s.on),
    }
}

/// "Unavailable" when no member is online, else "All on", "All off" or "N of M on".
pub fn light_text(s: &GroupSummary) -> String {
    if !s.available {
        UNAVAILABLE.into()
    } else if s.on == s.total {
        "All on".into()
    } else if s.on == 0 {
        "All off".into()
    } else {
        format!("{} of {} on", s.on, s.total)
    }
}

/// The members `cmd` goes to: the available ones, and for a position or a brightness only those
/// that support it.
pub fn targets(members: &[EntityView], cmd: &Command) -> Vec<String> {
    members
        .iter()
        .filter(|m| is_available(m))
        .filter(|m| match cmd {
            Command::SetPosition { .. } => m.capabilities.position,
            Command::TurnOn {
                brightness: Some(_),
                ..
            } => m.capabilities.brightness,
            _ => true,
        })
        .map(|m| m.id.clone())
        .collect()
}

/// The light group's toggle is absolute: off when any light is on, else on.
pub fn light_toggle(s: &GroupSummary) -> Command {
    if s.on > 0 {
        Command::TurnOff
    } else {
        Command::TurnOn {
            brightness: None,
            color: None,
            effect: None,
        }
    }
}

/// The rounded mean of `values`, `None` when there are none.
fn mean(values: impl Iterator<Item = u8>) -> Option<u8> {
    let (sum, n) = values.fold((0u32, 0u32), |(s, n), v| (s + u32::from(v), n + 1));
    (n > 0).then(|| (f64::from(sum) / f64::from(n)).round() as u8)
}

/// The "All to" slider's start: the mean of the known positions of position-capable members.
pub fn mean_position(members: &[EntityView]) -> u8 {
    mean(
        members
            .iter()
            .filter(|m| m.capabilities.position)
            .filter_map(|m| match &m.state.value {
                StateValue::Cover(c) => c.position.map(|p| p.min(100)),
                _ => None,
            }),
    )
    .unwrap_or(0)
}

/// The brightness slider's start: the mean brightness of the dimmable lights that are on.
pub fn mean_brightness(members: &[EntityView]) -> u8 {
    mean(
        members
            .iter()
            .filter(|m| m.capabilities.brightness)
            .filter_map(|m| match &m.state.value {
                StateValue::Light(l) if l.on => l.brightness,
                _ => None,
            }),
    )
    .unwrap_or(255)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::home::FAVORITES;
    use oikos_core::model::{
        Availability, Capabilities, CoverState, EntityCategory, EntityState, Kind, LightState,
        StateValue, UNASSIGNED_AREA, UiOverride,
    };

    fn entity(id: &str, kind: Kind, area: &str, ui: UiOverride) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind,
            component: kind.component().to_string(),
            protocol_kind: kind,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: area.into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui,
            state: EntityState::default(),
        }
    }

    fn light(id: &str, area: &str) -> EntityView {
        entity(id, Kind::Light, area, UiOverride::default())
    }

    fn cover(id: &str, area: &str) -> EntityView {
        entity(id, Kind::Cover, area, UiOverride::default())
    }

    fn area(id: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: id.into(),
            icon: None,
            order: None,
            entity_count: 1,
            primary_count: 1,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }
    }

    fn favourite_groups(id: &str, kinds: &[&str]) -> AreaView {
        AreaView {
            group_favorites: kinds.iter().map(|k| (*k).to_owned()).collect(),
            ..area(id)
        }
    }

    fn key(area: &str, kind: GroupKind, members: &[&str]) -> HomeItem {
        HomeItem::Group {
            key: GroupKey {
                area: area.into(),
                kind,
            },
            members: members.iter().map(|m| (*m).to_owned()).collect(),
        }
    }

    fn single(id: &str) -> HomeItem {
        HomeItem::Entity(id.into())
    }

    fn with_cover(mut e: EntityView, state: Option<&str>, position: Option<u8>) -> EntityView {
        e.state.value = StateValue::Cover(CoverState {
            state: state.map(Into::into),
            position,
        });
        e
    }

    fn with_light(mut e: EntityView, on: bool, brightness: Option<u8>) -> EntityView {
        e.state.value = StateValue::Light(LightState {
            on,
            brightness,
            ..Default::default()
        });
        e
    }

    fn open(id: &str) -> EntityView {
        with_cover(cover(id, "living"), Some("open"), None)
    }

    fn closed(id: &str) -> EntityView {
        with_cover(cover(id, "living"), Some("closed"), None)
    }

    fn no_children() -> BTreeMap<String, EntityView> {
        BTreeMap::new()
    }

    fn items(all: &[EntityView], areas: &[AreaView], chip: &str) -> Vec<HomeItem> {
        home_items(all.iter(), areas, chip)
    }

    #[test]
    fn two_lights_fold_into_one_group() {
        let all = [
            light("lamp_a", "living"),
            light("lamp_b", "living"),
            entity("plug", Kind::Switch, "living", UiOverride::default()),
        ];
        assert_eq!(
            items(&all, &[area("living")], "living"),
            [
                key("living", GroupKind::Light, &["lamp_a", "lamp_b"]),
                single("plug")
            ]
        );
    }

    #[test]
    fn ungrouped_member_dissolves_a_pair() {
        let all = [
            light("lamp_a", "living"),
            entity(
                "lamp_b",
                Kind::Light,
                "living",
                UiOverride {
                    ungrouped: true,
                    ..Default::default()
                },
            ),
            entity("plug", Kind::Switch, "living", UiOverride::default()),
        ];
        assert_eq!(
            items(&all, &[area("living")], "living"),
            [single("lamp_a"), single("lamp_b"), single("plug")]
        );
    }

    #[test]
    fn unassigned_never_groups() {
        let all = [light("a", UNASSIGNED_AREA), light("b", UNASSIGNED_AREA)];
        assert_eq!(
            items(&all, &[area(UNASSIGNED_AREA)], UNASSIGNED_AREA),
            [single("a"), single("b")]
        );
    }

    #[test]
    fn hidden_and_energy_members_leave() {
        let hidden = entity(
            "blind_b",
            Kind::Cover,
            "living",
            UiOverride {
                hidden: true,
                ..Default::default()
            },
        );
        let mut grid = light("lamp_b", "living");
        grid.ui.energy_role = Some("grid".into());
        let all = [
            cover("blind_a", "living"),
            hidden,
            light("lamp_a", "living"),
            grid,
        ];
        assert_eq!(
            items(&all, &[area("living")], "living"),
            [single("blind_a"), single("lamp_a")]
        );
    }

    #[test]
    fn group_takes_first_slot_and_goes_before_singles() {
        let pinned = entity(
            "zplug",
            Kind::Switch,
            "living",
            UiOverride {
                order: Some(0),
                ..Default::default()
            },
        );
        let all = [
            entity("aplug", Kind::Switch, "living", UiOverride::default()),
            light("lamp_a", "living"),
            pinned,
            light("lamp_b", "living"),
        ];
        assert_eq!(
            items(&all, &[area("living")], "living"),
            [
                single("zplug"),
                key("living", GroupKind::Light, &["lamp_a", "lamp_b"]),
                single("aplug")
            ]
        );
    }

    #[test]
    fn favorites_take_favourited_groups_whole() {
        let mut fav_lamp = light("lamp_a", "living");
        fav_lamp.ui.favorite = true;
        let all = [
            cover("blind_a", "living"),
            cover("blind_b", "living"),
            fav_lamp,
            light("lamp_b", "living"),
        ];
        let areas = [favourite_groups("living", &["cover"])];
        assert_eq!(
            items(&all, &areas, FAVORITES),
            [
                key("living", GroupKind::Cover, &["blind_a", "blind_b"]),
                single("lamp_a")
            ]
        );
    }

    fn favourite(mut e: EntityView) -> EntityView {
        e.ui.favorite = true;
        e
    }

    #[test]
    fn individually_favourited_members_fold_under_favorites() {
        let all = [
            favourite(cover("blind_a", "living")),
            favourite(cover("blind_b", "living")),
        ];
        assert_eq!(
            items(&all, &[area("living")], FAVORITES),
            [key("living", GroupKind::Cover, &["blind_a", "blind_b"])]
        );
        assert_eq!(favorites_count(all.iter(), &[area("living")]), 1);
    }

    #[test]
    fn partial_favourites_fold_to_a_partial_group() {
        let all = [
            favourite(light("lamp_a", "living")),
            favourite(light("lamp_b", "living")),
            light("lamp_c", "living"),
        ];
        assert_eq!(
            items(&all, &[area("living")], FAVORITES),
            [key("living", GroupKind::Light, &["lamp_a", "lamp_b"])]
        );
        // The room chip still shows the whole group.
        assert_eq!(
            items(&all, &[area("living")], "living"),
            [key(
                "living",
                GroupKind::Light,
                &["lamp_a", "lamp_b", "lamp_c"]
            )]
        );
    }

    #[test]
    fn single_favourite_member_stays_single() {
        let all = [
            favourite(light("lamp_a", "living")),
            light("lamp_b", "living"),
            light("lamp_c", "living"),
        ];
        assert_eq!(
            items(&all, &[area("living")], FAVORITES),
            [single("lamp_a")]
        );
    }

    #[test]
    fn favorites_order_follows_area_index() {
        let all = [
            light("a1", "attic"),
            light("a2", "attic"),
            light("b1", "bath"),
            light("b2", "bath"),
        ];
        let areas = [
            favourite_groups("bath", &["light"]),
            favourite_groups("attic", &["light"]),
        ];
        assert_eq!(
            items(&all, &areas, FAVORITES),
            [
                key("bath", GroupKind::Light, &["b1", "b2"]),
                key("attic", GroupKind::Light, &["a1", "a2"])
            ]
        );
    }

    #[test]
    fn favorites_chip_counts_favourited_groups() {
        let all = [light("a1", "attic"), light("a2", "attic")];
        let areas = [favourite_groups("attic", &["light"])];
        assert_eq!(favorites_count(all.iter(), &areas), 1);
        assert_eq!(favorites_count(all.iter(), &[area("attic")]), 0);
    }

    #[test]
    fn members_are_empty_below_two() {
        let all = [light("a", "living"), cover("c", "living")];
        let map: BTreeMap<_, _> = all.iter().map(|e| (e.id.clone(), e.clone())).collect();
        let k = GroupKey {
            area: "living".into(),
            kind: GroupKind::Light,
        };
        assert!(members(&map, &k).is_empty());
    }

    #[test]
    fn names_and_favourite_flag() {
        let mut a = area("living");
        a.group_names.insert("cover".into(), "  Blinds ".into());
        a.group_names.insert("light".into(), "   ".into());
        a.group_favorites.insert("cover".into());
        let cover_key = GroupKey {
            area: "living".into(),
            kind: GroupKind::Cover,
        };
        let light_key = GroupKey {
            area: "living".into(),
            kind: GroupKind::Light,
        };
        let areas = [a];
        assert_eq!(group_name(&areas, &cover_key), "Blinds");
        assert_eq!(group_name(&areas, &light_key), "Lights");
        assert_eq!(group_name(&[], &cover_key), "Shutters");
        assert!(is_favorite(&areas, &cover_key));
        assert!(!is_favorite(&areas, &light_key));
    }

    #[test]
    fn offline_groups_read_unavailable() {
        let mut a = open("a");
        let mut b = open("b");
        a.state.availability = Availability::Offline;
        b.state.availability = Availability::Offline;
        let s = summary(&[a, b], &no_children());
        assert_eq!(cover_text(&s), "Unavailable");
        assert_eq!(light_text(&s), "Unavailable");
    }

    #[test]
    fn cover_texts() {
        let all = [open("a"), open("b")];
        let s = summary(&all, &no_children());
        assert_eq!(cover_text(&s), "All open");
        let all = [open("a"), open("b"), closed("c")];
        assert_eq!(
            cover_text(&summary(&all, &no_children())),
            "2 open · 1 closed"
        );
        let all = [closed("a"), closed("b")];
        assert_eq!(cover_text(&summary(&all, &no_children())), "All closed");
        let all = [
            with_cover(cover("a", "living"), Some("opening"), None),
            with_cover(cover("b", "living"), Some("Closing"), None),
        ];
        let s = summary(&all, &no_children());
        assert_eq!(s.moving, Some(Motion::Opening));
        assert_eq!(cover_text(&s), "Opening…");
        let all = [
            with_cover(cover("a", "living"), Some("closing"), None),
            open("b"),
        ];
        assert_eq!(cover_text(&summary(&all, &no_children())), "Closing…");
    }

    #[test]
    fn cover_positions() {
        let all = [
            with_cover(cover("a", "living"), None, Some(150)),
            open("b"),
            closed("c"),
        ];
        assert_eq!(summary(&all, &no_children()).positions, [100, 100, 0]);
    }

    #[test]
    fn folded_moving_child_makes_a_cover_move() {
        let mut child = entity(
            "a_moving",
            Kind::BinarySensor,
            "living",
            UiOverride::default(),
        );
        child.name = "Moving".into();
        child.part_of = Some("a".into());
        child.state.value = StateValue::Text("closing".into());
        let mut plain = child.clone();
        plain.id = "b_moving".into();
        plain.part_of = Some("b".into());
        plain.state.value = StateValue::OnOff(true);
        let map: BTreeMap<_, _> = [child, plain]
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect();
        let only_a = [open("a"), open("c")];
        assert_eq!(summary(&only_a, &map).moving, Some(Motion::Closing));
        let only_b = [open("b"), open("c")];
        assert_eq!(summary(&only_b, &map).moving, Some(Motion::Opening));
    }

    #[test]
    fn light_texts() {
        let on = |id| with_light(light(id, "living"), true, None);
        let off = |id| with_light(light(id, "living"), false, None);
        let s = summary(&[on("a"), on("b")], &no_children());
        assert_eq!(light_text(&s), "All on");
        let s = summary(&[off("a"), off("b")], &no_children());
        assert_eq!(light_text(&s), "All off");
        assert_eq!(
            light_toggle(&s),
            Command::TurnOn {
                brightness: None,
                color: None,
                effect: None
            }
        );
        let s = summary(&[on("a"), on("b"), off("c")], &no_children());
        assert_eq!(light_text(&s), "2 of 3 on");
        assert_eq!(light_toggle(&s), Command::TurnOff);
    }

    #[test]
    fn availability_is_any_member_online() {
        let mut a = light("a", "living");
        a.state.availability = Availability::Offline;
        let mut b = light("b", "living");
        b.state.availability = Availability::Online;
        assert!(summary(&[a.clone(), b], &no_children()).available);
        a.state.availability = Availability::Offline;
        assert!(!summary(&[a.clone(), a], &no_children()).available);
    }

    fn offline(mut e: EntityView) -> EntityView {
        e.state.availability = Availability::Offline;
        e
    }

    #[test]
    fn targets_skip_unavailable() {
        let all = [light("a", "living"), offline(light("b", "living"))];
        assert_eq!(targets(&all, &Command::TurnOff), ["a"]);
    }

    #[test]
    fn targets_empty_when_all_offline() {
        let all = [offline(light("a", "living")), offline(light("b", "living"))];
        assert!(targets(&all, &Command::TurnOff).is_empty());
    }

    #[test]
    fn set_position_only_to_capable() {
        let mut a = cover("a", "living");
        a.capabilities.position = true;
        let all = [a, cover("b", "living")];
        assert_eq!(targets(&all, &Command::SetPosition { position: 40 }), ["a"]);
        assert_eq!(targets(&all, &Command::Open), ["a", "b"]);
    }

    #[test]
    fn brightness_only_to_dimmable() {
        let mut a = light("a", "living");
        a.capabilities.brightness = true;
        let all = [a, light("b", "living")];
        let dim = Command::TurnOn {
            brightness: Some(80),
            color: None,
            effect: None,
        };
        let plain = Command::TurnOn {
            brightness: None,
            color: None,
            effect: None,
        };
        assert_eq!(targets(&all, &dim), ["a"]);
        assert_eq!(targets(&all, &plain), ["a", "b"]);
    }

    #[test]
    fn means() {
        let mut a = with_cover(cover("a", "living"), None, Some(20));
        a.capabilities.position = true;
        let mut b = with_cover(cover("b", "living"), None, Some(50));
        b.capabilities.position = true;
        let unknown = cover("c", "living");
        assert_eq!(mean_position(&[a, b, unknown.clone()]), 35);
        assert_eq!(mean_position(&[unknown]), 0);

        let dim = |id: &str, on, b| {
            let mut e = with_light(light(id, "living"), on, b);
            e.capabilities.brightness = true;
            e
        };
        let all = [
            dim("a", true, Some(100)),
            dim("b", true, Some(201)),
            dim("c", false, Some(9)),
        ];
        assert_eq!(mean_brightness(&all), 151);
        assert_eq!(mean_brightness(&[dim("a", false, Some(9))]), 255);
    }

    #[test]
    fn group_route_round_trips_accented_area() {
        let k = GroupKey {
            area: "bedroom3".into(),
            kind: GroupKind::Light,
        };
        assert_eq!(group_path(&k), "/group/r%C3%A9mi/light");
        assert_eq!(parse_group_path("bedroom3", "light"), Some(k));
        assert_eq!(parse_group_path("bedroom3", "sensor"), None);
    }
}
