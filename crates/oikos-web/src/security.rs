//! Pure presentation of the Security tab: entities and areas → panels, actions, sensor rows.

use std::collections::BTreeMap;

use oikos_api::{AreaView, EntityView};
use oikos_core::model::{
    Availability, Capabilities, Command, Kind, Placement, SensorGroup, SensorSection, StateValue,
    UNASSIGNED_AREA,
};

/// An alarm panel. `state` is the raw alarm state string; `None`: nothing received yet.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelUi {
    pub id: String,
    pub name: String,
    pub state: Option<String>,
    pub available: bool,
    pub capabilities: Capabilities,
    pub order: Option<i32>,
}

/// A security binary sensor. `active` is `None` until a state arrives; `area_name` is `None`
/// for Unassigned.
#[derive(Clone, Debug, PartialEq)]
pub struct SensorRow {
    pub id: String,
    pub name: String,
    pub area_name: Option<String>,
    pub device_class: Option<String>,
    pub group: SensorGroup,
    pub active: Option<bool>,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SectionUi {
    pub section: SensorSection,
    pub rows: Vec<SensorRow>,
}

/// A switch or button opted onto Security. `on` is `None` for a button or before a state.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionRow {
    pub id: String,
    pub name: String,
    pub area_name: Option<String>,
    pub kind: Kind,
    pub on: Option<bool>,
    pub available: bool,
    pub ask: bool,
}

/// What a tap on an action does, and whether it asks first.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionTap {
    pub command: Command,
    pub confirm: bool,
}

impl ActionRow {
    pub fn tap(&self) -> ActionTap {
        match (self.kind, self.on) {
            (Kind::Switch, Some(true)) => ActionTap {
                command: Command::TurnOff,
                confirm: false,
            },
            (Kind::Switch, _) => ActionTap {
                command: Command::TurnOn {
                    brightness: None,
                    color: None,
                    effect: None,
                },
                confirm: self.ask,
            },
            _ => ActionTap {
                command: Command::Press,
                confirm: self.ask,
            },
        }
    }

    /// "On" / "Off" / "Unavailable" / "—"; `None` for an available button, which has no state.
    pub fn state_word(&self) -> Option<&'static str> {
        if !self.available {
            return Some("Unavailable");
        }
        if self.kind == Kind::Button {
            return None;
        }
        Some(match self.on {
            Some(true) => "On",
            Some(false) => "Off",
            None => "—",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupCount {
    pub group: SensorGroup,
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecurityUi {
    pub panels: Vec<PanelUi>,
    pub summary: Vec<GroupCount>,
    pub sensors: Vec<SensorRow>,
    pub sections: Vec<SectionUi>,
    pub actions: Vec<ActionRow>,
}

/// The stable tile order: area name (Unassigned last), then name, then id.
type SortKey = (bool, String, String, String);

fn sort_key(area_name: &Option<String>, name: &str, id: &str) -> SortKey {
    (
        area_name.is_none(),
        area_name.as_deref().unwrap_or_default().to_lowercase(),
        name.to_lowercase(),
        id.to_owned(),
    )
}

fn area_name(e: &EntityView, names: &BTreeMap<&str, &str>) -> Option<String> {
    if e.area == UNASSIGNED_AREA {
        None
    } else {
        Some(
            names
                .get(e.area.as_str())
                .map_or_else(|| e.area.clone(), |n| (*n).to_owned()),
        )
    }
}

fn is_on(e: &EntityView) -> Option<bool> {
    match e.state.value {
        StateValue::OnOff(on) => Some(on),
        _ => None,
    }
}

fn is_available(e: &EntityView) -> bool {
    e.state.availability != Availability::Offline
}

impl SecurityUi {
    pub fn build(entities: &BTreeMap<String, EntityView>, areas: &[AreaView]) -> SecurityUi {
        let names: BTreeMap<&str, &str> = areas
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str()))
            .collect();
        // Hidden on Home still shows here: only `securityHidden` takes an entity off Security.
        let visible: Vec<&EntityView> = entities
            .values()
            .filter(|e| {
                e.part_of.is_none()
                    && Placement::on_security(e.kind, e.device_class.as_deref(), &e.ui)
            })
            .collect();

        let mut panels: Vec<PanelUi> = visible
            .iter()
            .filter(|e| e.kind == Kind::AlarmPanel)
            .map(|e| PanelUi {
                id: e.id.clone(),
                name: e.name.clone(),
                state: match &e.state.value {
                    StateValue::Alarm(s) => Some(s.clone()),
                    _ => None,
                },
                available: is_available(e),
                capabilities: e.capabilities.clone(),
                order: e.ui.order,
            })
            .collect();
        panels.sort_by_cached_key(|p| {
            (
                p.order.unwrap_or(i32::MAX),
                p.name.to_lowercase(),
                p.id.clone(),
            )
        });

        let mut sensors: Vec<SensorRow> = visible
            .iter()
            .filter(|e| e.kind == Kind::BinarySensor)
            .filter_map(|e| {
                let group = SensorGroup::of(e.device_class.as_deref())?;
                Some(SensorRow {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    area_name: area_name(e, &names),
                    device_class: e.device_class.clone(),
                    group,
                    active: is_on(e),
                    available: is_available(e),
                })
            })
            .collect();
        sensors.sort_by_cached_key(|s| sort_key(&s.area_name, &s.name, &s.id));

        let mut actions: Vec<ActionRow> = visible
            .iter()
            .filter(|e| Placement::action_kind(e.kind))
            .map(|e| ActionRow {
                id: e.id.clone(),
                name: e.name.clone(),
                area_name: area_name(e, &names),
                kind: e.kind,
                on: is_on(e),
                available: is_available(e),
                ask: e.ui.security_ask,
            })
            .collect();
        actions.sort_by_cached_key(|a| sort_key(&a.area_name, &a.name, &a.id));

        let summary = SensorGroup::ALL
            .iter()
            .filter_map(|&group| {
                let count = sensors
                    .iter()
                    .filter(|s| s.group == group && s.available && s.active == Some(true))
                    .count();
                (count > 0).then_some(GroupCount { group, count })
            })
            .collect();

        let sections = SensorSection::ALL
            .iter()
            .filter_map(|&section| {
                let rows: Vec<SensorRow> = sensors
                    .iter()
                    .filter(|s| SensorSection::of(s.group) == section)
                    .cloned()
                    .collect();
                (!rows.is_empty()).then_some(SectionUi { section, rows })
            })
            .collect();

        SecurityUi {
            panels,
            summary,
            sensors,
            sections,
            actions,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.panels.is_empty() && self.sensors.is_empty() && self.actions.is_empty()
    }

    /// "1 smoke · 2 open · 1 motion", or "All clear".
    pub fn summary_text(&self) -> String {
        if self.summary.is_empty() {
            return "All clear".to_owned();
        }
        self.summary
            .iter()
            .map(|c| format!("{} {}", c.count, c.group.word()))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl SensorRow {
    pub fn state_word(&self) -> &'static str {
        if !self.available {
            return "Unavailable";
        }
        let Some(active) = self.active else {
            return "—";
        };
        let (on, off) = match self.group {
            SensorGroup::Opening => ("Open", "Closed"),
            SensorGroup::Motion => ("Motion", "Clear"),
            SensorGroup::Smoke | SensorGroup::Co | SensorGroup::Gas => ("Detected", "Clear"),
            SensorGroup::Leak => ("Leak", "Dry"),
            SensorGroup::Sos => ("SOS", "OK"),
            SensorGroup::Vibration => ("Vibration", "Still"),
            SensorGroup::Tamper => ("Tampered", "OK"),
        };
        if active { on } else { off }
    }

    fn active_now(&self) -> bool {
        self.available && self.active == Some(true)
    }

    pub fn danger_active(&self) -> bool {
        self.active_now() && self.group.danger()
    }

    pub fn intrusion_active(&self) -> bool {
        self.active_now() && !self.group.danger()
    }

    /// "Garage door, garage, open".
    pub fn aria_label(&self) -> String {
        let word = self.state_word().to_lowercase();
        match &self.area_name {
            Some(area) => format!("{}, {}, {}", self.name, area.to_lowercase(), word),
            None => format!("{}, {}", self.name, word),
        }
    }
}

/// "Disarmed", "Armed · Away", "Exit delay…", "Entry delay…", "Triggered!", "Disarming…",
/// "Unavailable", "—".
pub fn panel_state_text(state: Option<&str>, available: bool) -> String {
    if !available {
        return "Unavailable".to_owned();
    }
    match state {
        Some("disarmed") => "Disarmed",
        Some("armed_home") => "Armed · Home",
        Some("armed_away") => "Armed · Away",
        Some("armed_night") => "Armed · Night",
        Some("armed_vacation") => "Armed · Vacation",
        Some("armed_custom_bypass") => "Armed · Custom",
        Some("arming") => "Exit delay…",
        Some("pending") => "Entry delay…",
        Some("disarming") => "Disarming…",
        Some("triggered") => "Triggered!",
        _ => "—",
    }
    .to_owned()
}

/// The hero's modifier class: "disarmed", "armed", "arming", "pending", "triggered",
/// "disarming" or "off" (unavailable or unknown).
pub fn panel_tone(state: Option<&str>, available: bool) -> &'static str {
    if !available {
        return "off";
    }
    match state {
        Some("disarmed") => "disarmed",
        Some(s) if s.starts_with("armed_") => "armed",
        Some("arming") => "arming",
        Some("pending") => "pending",
        Some("triggered") => "triggered",
        Some("disarming") => "disarming",
        _ => "off",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oikos_api::{AreaView, EntityView};
    use oikos_core::model::{
        Capabilities, Command, EntityCategory, EntityState, Kind, SensorSection, StateValue,
        UiOverride,
    };

    use super::{SecurityUi, panel_state_text, panel_tone};

    fn entity(
        id: &str,
        kind: Kind,
        class: Option<&str>,
        area: &str,
        value: StateValue,
    ) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind,
            protocol_kind: kind,
            device_class: class.map(Into::into),
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: area.into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState {
                value,
                ..Default::default()
            },
        }
    }

    fn door(id: &str, area: &str, open: bool) -> EntityView {
        entity(
            id,
            Kind::BinarySensor,
            Some("door"),
            area,
            StateValue::OnOff(open),
        )
    }

    fn house(list: Vec<EntityView>) -> BTreeMap<String, EntityView> {
        list.into_iter().map(|e| (e.id.clone(), e)).collect()
    }

    fn area(id: &str, name: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: name.into(),
            icon: None,
            order: None,
            entity_count: 0,
            primary_count: 0,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }
    }

    fn build(list: Vec<EntityView>) -> SecurityUi {
        SecurityUi::build(&house(list), &[area("garage", "Garage")])
    }

    #[test]
    fn filters_by_placement() {
        let mut hidden = door("hidden_door", "garage", false);
        hidden.ui.hidden = true;
        let mut off_security = door("off_door", "garage", false);
        off_security.ui.security_hidden = true;
        let mut folded = door("folded_door", "garage", false);
        folded.part_of = Some("parent".into());
        let plain_switch = entity(
            "plain",
            Kind::Switch,
            None,
            "garage",
            StateValue::OnOff(false),
        );
        let mut siren = entity(
            "siren",
            Kind::Switch,
            None,
            "garage",
            StateValue::OnOff(false),
        );
        siren.ui.security_action = true;
        let ui = build(vec![hidden, off_security, folded, plain_switch, siren]);
        let ids: Vec<_> = ui.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["hidden_door"]);
        let actions: Vec<_> = ui.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(actions, ["siren"]);
    }

    #[test]
    fn stable_order_area_then_name() {
        let ui = build(vec![
            door("b", "Unassigned", false),
            door("z", "garage", false),
            door("a", "garage", true),
        ]);
        let ids: Vec<_> = ui.sensors.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a", "z", "b"]);
        assert_eq!(ui.sensors[0].area_name.as_deref(), Some("Garage"));
        assert_eq!(ui.sensors[2].area_name, None);
    }

    #[test]
    fn summary_words() {
        let smoke = |id: &str, on| {
            entity(
                id,
                Kind::BinarySensor,
                Some("smoke"),
                "garage",
                StateValue::OnOff(on),
            )
        };
        let motion = entity(
            "m",
            Kind::BinarySensor,
            Some("motion"),
            "garage",
            StateValue::OnOff(true),
        );
        let mut offline = smoke("offline_smoke", true);
        offline.state.availability = oikos_core::model::Availability::Offline;
        let ui = build(vec![
            smoke("s1", true),
            smoke("s2", false),
            offline,
            door("d1", "garage", true),
            door("d2", "garage", true),
            motion,
        ]);
        assert_eq!(ui.summary_text(), "1 smoke · 2 open · 1 motion");
        assert_eq!(
            build(vec![door("d", "garage", false)]).summary_text(),
            "All clear"
        );
    }

    #[test]
    fn sections_in_order_without_empties() {
        let ui = build(vec![
            entity(
                "w",
                Kind::BinarySensor,
                Some("moisture"),
                "garage",
                StateValue::OnOff(false),
            ),
            door("d", "garage", false),
            entity(
                "t",
                Kind::BinarySensor,
                Some("tamper"),
                "garage",
                StateValue::OnOff(false),
            ),
        ]);
        let sections: Vec<_> = ui.sections.iter().map(|s| s.section).collect();
        assert_eq!(
            sections,
            [
                SensorSection::Openings,
                SensorSection::Water,
                SensorSection::Other
            ]
        );
    }

    #[test]
    fn state_words_per_group() {
        let word = |class: &str, on: bool| {
            let ui = build(vec![entity(
                "x",
                Kind::BinarySensor,
                Some(class),
                "garage",
                StateValue::OnOff(on),
            )]);
            ui.sensors[0].state_word()
        };
        assert_eq!(
            (word("door", true), word("door", false)),
            ("Open", "Closed")
        );
        assert_eq!(
            (word("motion", true), word("motion", false)),
            ("Motion", "Clear")
        );
        assert_eq!(
            (word("gas", true), word("smoke", false)),
            ("Detected", "Clear")
        );
        assert_eq!(
            (word("moisture", true), word("moisture", false)),
            ("Leak", "Dry")
        );
        assert_eq!((word("safety", true), word("safety", false)), ("SOS", "OK"));
        assert_eq!(
            (word("vibration", true), word("vibration", false)),
            ("Vibration", "Still")
        );
        assert_eq!(
            (word("tamper", true), word("tamper", false)),
            ("Tampered", "OK")
        );
        let unknown = entity(
            "u",
            Kind::BinarySensor,
            Some("door"),
            "garage",
            StateValue::Unknown,
        );
        assert_eq!(build(vec![unknown]).sensors[0].state_word(), "—");
        let mut offline = door("o", "garage", true);
        offline.state.availability = oikos_core::model::Availability::Offline;
        let ui = build(vec![offline]);
        assert_eq!(ui.sensors[0].state_word(), "Unavailable");
        assert!(!ui.sensors[0].danger_active() && !ui.sensors[0].intrusion_active());
    }

    #[test]
    fn aria_label_names_area_and_state() {
        let mut d = door("Garage door", "garage", true);
        d.name = "Garage door".into();
        let ui = build(vec![d]);
        assert_eq!(ui.sensors[0].aria_label(), "Garage door, garage, open");
        assert!(ui.sensors[0].intrusion_active());
    }

    fn action(kind: Kind, value: StateValue) -> super::ActionRow {
        let mut e = entity("a", kind, None, "garage", value);
        e.ui.security_action = true;
        build(vec![e]).actions.remove(0)
    }

    #[test]
    fn button_tap_asks_by_default() {
        let row = action(Kind::Button, StateValue::Unknown);
        let tap = row.tap();
        assert_eq!(tap.command, Command::Press);
        assert!(tap.confirm);
        let mut e = entity("a", Kind::Button, None, "garage", StateValue::Unknown);
        e.ui.security_action = true;
        e.ui.security_ask = false;
        assert!(!build(vec![e]).actions[0].tap().confirm);
    }

    #[test]
    fn button_has_no_state_word() {
        assert_eq!(action(Kind::Button, StateValue::Unknown).state_word(), None);
        assert_eq!(
            action(Kind::Switch, StateValue::OnOff(false)).state_word(),
            Some("Off")
        );
        assert_eq!(
            action(Kind::Switch, StateValue::OnOff(true)).state_word(),
            Some("On")
        );
    }

    #[test]
    fn switch_on_turns_off_without_asking() {
        let tap = action(Kind::Switch, StateValue::OnOff(true)).tap();
        assert_eq!(tap.command, Command::TurnOff);
        assert!(!tap.confirm);
        let tap = action(Kind::Switch, StateValue::OnOff(false)).tap();
        assert!(matches!(tap.command, Command::TurnOn { .. }));
        assert!(tap.confirm);
    }

    #[test]
    fn panel_text_and_tone() {
        assert_eq!(panel_state_text(Some("armed_away"), true), "Armed · Away");
        assert_eq!(panel_tone(Some("armed_away"), true), "armed");
        assert_eq!(panel_state_text(Some("triggered"), true), "Triggered!");
        assert_eq!(panel_tone(Some("triggered"), true), "triggered");
        assert_eq!(panel_state_text(None, true), "—");
        assert_eq!(panel_tone(None, true), "off");
        assert_eq!(panel_state_text(Some("disarmed"), false), "Unavailable");
        assert_eq!(panel_tone(Some("disarmed"), false), "off");
        assert_eq!(panel_state_text(Some("arming"), true), "Exit delay…");
        assert_eq!(panel_state_text(Some("pending"), true), "Entry delay…");
        assert_eq!(panel_state_text(Some("disarming"), true), "Disarming…");
    }

    #[test]
    fn panels_sorted_by_order_then_name() {
        let mut a = entity(
            "b",
            Kind::AlarmPanel,
            None,
            "garage",
            StateValue::Alarm("disarmed".into()),
        );
        a.ui.order = Some(2);
        let b = entity(
            "a",
            Kind::AlarmPanel,
            None,
            "garage",
            StateValue::Alarm("armed_home".into()),
        );
        let mut c = entity("c", Kind::AlarmPanel, None, "garage", StateValue::Unknown);
        c.ui.order = Some(1);
        let ui = build(vec![a, b, c]);
        let ids: Vec<_> = ui.panels.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["c", "b", "a"]);
        assert_eq!(ui.panels[0].state, None);
    }
}
