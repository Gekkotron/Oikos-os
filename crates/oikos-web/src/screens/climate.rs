//! The Climate screen: one card per zone, the house settings and the engine's banners.
//!
//! Only the zone list is keyed by zone id; the cards, the settings sheet and their local state
//! live outside the closures that react to a refetch, so a refresh never resets them.

use chrono_tz::Tz;
use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_climate::view::{ClimateView, ZoneView};

use crate::api::climate as api;
use crate::climate::parse_zone;
use crate::climate::settings_sheet::SettingsSheet;
use crate::climate::zone_card::ZoneCard;
use crate::house::House;

/// The zone ids of `view` in display order.
pub fn zone_ids(view: Option<&ClimateView>) -> Vec<String> {
    view.map(|v| v.zones.iter().map(|z| z.zone.clone()).collect())
        .unwrap_or_default()
}

/// The zone `id` in `view`.
pub fn find_zone(view: Option<&ClimateView>, id: &str) -> Option<ZoneView> {
    view?.zones.iter().find(|z| z.zone == id).cloned()
}

#[component]
pub fn ClimateScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let climate = RwSignal::new(None::<ClimateView>);
    let failed = RwSignal::new(None::<String>);
    let settings_open = RwSignal::new(false);

    let reload = Callback::new(move |_: ()| {
        spawn_local(async move {
            match api::get_climate().await {
                Ok(v) => {
                    failed.try_set(None);
                    climate.try_set(Some(v));
                }
                Err(e) => {
                    failed.try_set(Some(e.error));
                }
            }
        });
    });
    // Refetch on mount and whenever the hub says the engine changed.
    Effect::new(move |_| {
        house.climate_rev.track();
        reload.run(());
    });

    let ids = Memo::new(move |_| climate.with(|v| zone_ids(v.as_ref())));
    let tz = Signal::derive(move || {
        climate.with(|v| v.as_ref().map_or(Tz::UTC, |v| parse_zone(&v.timezone)))
    });
    let loaded = move || climate.with(|v| v.is_some());

    view! {
        <div class="climate">
            <header class="sheet__header">
                <div class="sheet__titles">
                    <h1 class="sheet__title">"Climate"</h1>
                </div>
                <button
                    type="button"
                    class="btn btn--compact"
                    on:click=move |_| settings_open.update(|o| *o = !*o)
                >
                    "Settings"
                </button>
            </header>
            {move || failed.get().map(|e| view! { <p class="badge climate__error">{e}</p> })}
            {move || {
                climate
                    .with(|v| v.as_ref().and_then(|v| v.house_invalid.clone()))
                    .map(|e| view! { <p class="badge">{e}</p> })
            }}
            {move || {
                (loaded() && !climate.with(|v| v.as_ref().is_some_and(|v| v.history)))
                    .then(|| view! {
                        <p class="badge climate__banner">
                            "History is off: rules can't be learned until a history database is set up."
                        </p>
                    })
            }}
            // Built when the sheet opens from the settings of that moment; a refetch leaves the
            // draft alone.
            <Show when=move || settings_open.get() && loaded()>
                {move || {
                    let current = climate.get_untracked().map(|v| v.house).unwrap_or_default();
                    view! {
                        <SettingsSheet
                            house=current
                            on_saved=Callback::new(move |_| {
                                settings_open.set(false);
                                reload.run(());
                            })
                            on_close=Callback::new(move |_| settings_open.set(false))
                        />
                    }
                }}
            </Show>
            <Show when=move || loaded() && ids.with(|i| i.is_empty())>
                <p class="climate__empty">"No room has a climate device"</p>
            </Show>
            <div class="climate__zones">
                <For each=move || ids.get() key=|id| id.clone() let:id>
                    {
                        let fallback = find_zone(climate.get_untracked().as_ref(), &id);
                        let zone = Memo::new(move |last: Option<&ZoneView>| {
                            find_zone(climate.get().as_ref(), &id)
                                .or_else(|| last.cloned())
                                .or_else(|| fallback.clone())
                                .expect("a keyed zone was in the view when its card was built")
                        });
                        view! { <ZoneCard zone=zone.into() tz=tz on_changed=reload /> }
                    }
                </For>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_climate::doc::{HouseSettings, ZoneSettings};
    use oikos_climate::view::LearnStatus;

    fn zone(id: &str, reading: Option<f64>) -> ZoneView {
        ZoneView {
            zone: id.into(),
            name: id.into(),
            orphan: false,
            settings: ZoneSettings::default(),
            settings_invalid: None,
            entities: vec![],
            reading,
            reading_age_s: None,
            room_sensor: None,
            decision: None,
            overrides: vec![],
            proposal: None,
            learn: LearnStatus::Off,
        }
    }

    fn view(zones: Vec<ZoneView>) -> ClimateView {
        ClimateView {
            enabled: true,
            house: HouseSettings::default(),
            house_invalid: None,
            history: true,
            timezone: "Europe/Paris".into(),
            zones,
        }
    }

    #[test]
    fn ids_keep_order_and_find_reads_the_fresh_zone() {
        let v = view(vec![zone("a", Some(19.0)), zone("b", None)]);
        assert_eq!(zone_ids(Some(&v)), ["a", "b"]);
        assert!(zone_ids(None).is_empty());
        assert_eq!(find_zone(Some(&v), "a").unwrap().reading, Some(19.0));
        assert!(find_zone(Some(&v), "gone").is_none());
    }
}
