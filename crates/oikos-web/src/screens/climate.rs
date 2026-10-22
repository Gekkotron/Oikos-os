//! The Climate screen: one card per zone, the house settings and the engine's banners.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_climate::view::ClimateView;

use crate::api::climate as api;
use crate::climate::settings_sheet::SettingsSheet;
use crate::climate::zone_card::ZoneCard;
use crate::house::House;

#[component]
pub fn ClimateScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let view_signal = RwSignal::new(None::<ClimateView>);
    let failed = RwSignal::new(None::<String>);
    let settings_open = RwSignal::new(false);

    let reload = Callback::new(move |_: ()| {
        spawn_local(async move {
            match api::get_climate().await {
                Ok(v) => {
                    failed.try_set(None);
                    view_signal.try_set(Some(v));
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
                view_signal
                    .get()
                    .map(|v| {
                        let history = v.history;
                        let house_settings = v.house.clone();
                        view! {
                            {v.house_invalid.clone().map(|e| view! { <p class="badge">{e}</p> })}
                            {(!history)
                                .then(|| view! {
                                    <p class="badge climate__banner">
                                        "History is off: rules can't be learned until a history database is set up."
                                    </p>
                                })}
                            <Show when=move || settings_open.get()>
                                {
                                    let house_settings = house_settings.clone();
                                    view! {
                                        <SettingsSheet
                                            house=house_settings
                                            on_saved=Callback::new(move |_| {
                                                settings_open.set(false);
                                                reload.run(());
                                            })
                                            on_close=Callback::new(move |_| settings_open.set(false))
                                        />
                                    }
                                }
                            </Show>
                            {if v.zones.is_empty() {
                                view! { <p class="climate__empty">"No room has a climate device"</p> }
                                    .into_any()
                            } else {
                                view! {
                                    <div class="climate__zones">
                                        {v
                                            .zones
                                            .into_iter()
                                            .map(|z| view! { <ZoneCard zone=z on_changed=reload /> })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }}
                        }
                    })
            }}
        </div>
    }
}
