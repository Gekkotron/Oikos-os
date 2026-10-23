//! Which sidebar entry is active for a path, and the breadcrumb trail of a page.

use crate::i18n::{Key, Lang, text};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Overview,
    Room(String),
    Automations,
    Groups,
    Energy,
    Security,
    Climate,
    Alerts,
    Settings,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookup<'a> {
    Dashboard(&'a str),
    Room(&'a str),
    Flow(&'a str),
    Group(&'a str),
    Entity(&'a str),
    Alert(&'a str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub href: Option<String>,
}

/// The path's segments, ignoring any query, fragment and empty segments.
fn segments(path: &str) -> Vec<&str> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn is_detail_page(path: &str) -> bool {
    matches!(segments(path).first(), Some(&"entity") | Some(&"history"))
}

/// The sidebar entry to highlight. Entity and history pages inherit the section of the page the
/// user came from (the latest non-entity/history entry of `back`).
pub fn section_for(path: &str, back: &[String]) -> Section {
    let seg = segments(path);
    match seg.as_slice() {
        [] | ["dashboards", ..] => Section::Overview,
        ["rooms", id, ..] => Section::Room((*id).to_string()),
        ["flows", ..] => Section::Automations,
        ["groups", ..] | ["group", ..] => Section::Groups,
        ["energy", ..] => Section::Energy,
        ["security", ..] => Section::Security,
        ["climate", ..] => Section::Climate,
        ["alerts", ..] => Section::Alerts,
        ["settings", ..] | ["devices", ..] => Section::Settings,
        ["entity", ..] | ["history", ..] => back
            .iter()
            .rev()
            .find(|p| !is_detail_page(p))
            .map_or(Section::None, |p| section_for(p, &[])),
        _ => Section::None,
    }
}

/// The breadcrumb trail: always starts at "My home" and the last crumb has no link.
pub fn crumbs(path: &str, lang: Lang, name_of: impl Fn(Lookup) -> Option<String>) -> Vec<Crumb> {
    let t = |k| text(lang, k).to_string();
    let named = |l: Lookup, id: &str| name_of(l).unwrap_or_else(|| id.to_string());
    let link = |label: String, href: &str| Crumb {
        label,
        href: Some(href.to_string()),
    };
    let mut out = vec![link(t(Key::MyHome), "/")];
    let seg = segments(path);
    let mut push = |label: String, href: String| out.push(link(label, &href));
    match seg.as_slice() {
        ["dashboards", "new"] => push(t(Key::NewDashboard), String::new()),
        ["dashboards", id, ..] => push(named(Lookup::Dashboard(id), id), String::new()),
        ["rooms", id, ..] => push(named(Lookup::Room(id), id), String::new()),
        ["flows"] => push(t(Key::Automations), String::new()),
        ["flows", rest @ ..] => {
            push(t(Key::Automations), "/flows".into());
            match rest {
                ["new"] => push(t(Key::NewFlow), String::new()),
                [id, ..] => push(named(Lookup::Flow(id), id), String::new()),
                [] => {}
            }
        }
        ["groups"] => push(t(Key::Groups), String::new()),
        ["groups", "new"] => {
            push(t(Key::Groups), "/groups".into());
            push(t(Key::NewGroup), String::new());
        }
        ["groups", id, "edit"] | ["group", id, "edit"] => {
            push(t(Key::Groups), "/groups".into());
            push(named(Lookup::Group(id), id), format!("/group/{id}"));
            push(t(Key::EditGroup), String::new());
        }
        ["groups", id, ..] | ["group", id, ..] => {
            push(t(Key::Groups), "/groups".into());
            push(named(Lookup::Group(id), id), String::new());
        }
        ["energy", ..] => push(t(Key::Energy), String::new()),
        ["security", ..] => push(t(Key::Security), String::new()),
        ["climate", ..] => push(t(Key::Climate), String::new()),
        ["alerts"] => push(t(Key::Alerts), String::new()),
        ["alerts", id, ..] => {
            push(t(Key::Alerts), "/alerts".into());
            push(
                name_of(Lookup::Alert(id)).unwrap_or_else(|| t(Key::Alert)),
                String::new(),
            );
        }
        ["settings", ..] => push(t(Key::Settings), String::new()),
        ["devices", ..] => push(t(Key::Devices), String::new()),
        ["entity", id, ..] => push(named(Lookup::Entity(id), id), String::new()),
        ["history", id, ..] => {
            push(named(Lookup::Entity(id), id), format!("/entity/{id}"));
            push(t(Key::History), String::new());
        }
        _ => {}
    }
    if let Some(last) = out.last_mut() {
        last.href = None;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(l: Lookup) -> Option<String> {
        match l {
            Lookup::Dashboard("d1") => Some("Wall tablet".into()),
            Lookup::Room("r1") => Some("Kitchen".into()),
            Lookup::Flow("f1") => Some("Night lights".into()),
            Lookup::Entity("light.a") => Some("Lamp".into()),
            _ => None,
        }
    }
    fn labels(c: &[Crumb]) -> Vec<&str> {
        c.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn sections() {
        assert_eq!(section_for("/", &[]), Section::Overview);
        assert_eq!(section_for("/dashboards/house", &[]), Section::Overview);
        assert_eq!(section_for("/rooms/r1", &[]), Section::Room("r1".into()));
        assert_eq!(section_for("/flows/f1", &[]), Section::Automations);
        assert_eq!(section_for("/group/g1", &[]), Section::Groups);
        assert_eq!(section_for("/groups/g1/edit", &[]), Section::Groups);
        assert_eq!(section_for("/alerts/a1", &[]), Section::Alerts);
        assert_eq!(section_for("/devices", &[]), Section::Settings);
        let back = vec!["/rooms/r1".to_string(), "/entity/light.a".to_string()];
        assert_eq!(
            section_for("/entity/light.a", &back),
            Section::Room("r1".into())
        );
        assert_eq!(section_for("/history/light.a", &[]), Section::None);
    }

    #[test]
    fn breadcrumbs() {
        assert_eq!(
            labels(&crumbs("/rooms/r1", Lang::Fr, names)),
            ["Ma maison", "Kitchen"]
        );
        assert_eq!(
            labels(&crumbs("/flows/f1", Lang::Fr, names)),
            ["Ma maison", "Automatisations", "Night lights"]
        );
        assert_eq!(
            labels(&crumbs("/flows/new", Lang::En, names)),
            ["My home", "Automations", "New flow"]
        );
        assert_eq!(
            labels(&crumbs("/dashboards/d1", Lang::En, names)),
            ["My home", "Wall tablet"]
        );
        assert_eq!(
            labels(&crumbs("/history/light.a", Lang::En, names)),
            ["My home", "Lamp", "History"]
        );
        assert_eq!(
            labels(&crumbs("/rooms/zz", Lang::En, names)),
            ["My home", "zz"]
        );
        let c = crumbs("/flows/f1", Lang::En, names);
        assert_eq!(c[0].href.as_deref(), Some("/"));
        assert_eq!(c[1].href.as_deref(), Some("/flows"));
        assert_eq!(c[2].href, None);
    }

    #[test]
    fn extra_crumbs() {
        assert_eq!(labels(&crumbs("/", Lang::En, names)), ["My home"]);
        assert_eq!(crumbs("/", Lang::En, names)[0].href, None);
        assert_eq!(
            labels(&crumbs("/alerts/zz", Lang::Fr, names)),
            ["Ma maison", "Alertes", "Alerte"]
        );
        let c = crumbs("/groups/g1/edit", Lang::En, names);
        assert_eq!(labels(&c), ["My home", "Groups", "g1", "Edit group"]);
        assert_eq!(c[2].href.as_deref(), Some("/group/g1"));
    }
}
