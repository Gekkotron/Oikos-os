//! Inline SVG icons, one per kind plus device-class overrides. 24x24 line icons that take the
//! current text colour.

use oikos_api::AlertChannel;
use oikos_core::model::Kind;

macro_rules! svg {
    ($body:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">"#,
            $body,
            "</svg>"
        )
    };
}

const LIGHT: &str = svg!(
    r#"<path d="M9 18h6"/><path d="M10 21h4"/><path d="M12 3a6 6 0 0 0-3.5 10.9c.6.5 1 1.2 1 2.1h5c0-.9.4-1.6 1-2.1A6 6 0 0 0 12 3z"/>"#
);
const SWITCH: &str = svg!(
    r#"<rect x="2.5" y="7" width="19" height="10" rx="5"/><circle cx="16.5" cy="12" r="2.5"/>"#
);
const COVER: &str = svg!(
    r#"<path d="M4 4h16"/><path d="M5 4v6h14V4"/><path d="M5 10v2h14v-2"/><path d="M12 14v6"/><path d="M9.5 17.5 12 20l2.5-2.5"/>"#
);
const CLIMATE: &str =
    svg!(r#"<path d="M14 14.8V5a2 2 0 0 0-4 0v9.8a4 4 0 1 0 4 0z"/><path d="M12 9v7"/>"#);
const SENSOR: &str = svg!(r#"<path d="M3 12h4l3-7 4 14 3-7h4"/>"#);
const BINARY_SENSOR: &str = svg!(
    r#"<circle cx="12" cy="12" r="3"/><path d="M6.3 6.3a8 8 0 0 0 0 11.4"/><path d="M17.7 6.3a8 8 0 0 1 0 11.4"/>"#
);
const ALARM: &str =
    svg!(r#"<path d="M12 3 5 6v5c0 4.5 3 8 7 10 4-2 7-5.5 7-10V6z"/><path d="m9 12 2 2 4-4"/>"#);
const NUMBER: &str =
    svg!(r#"<path d="M4 8h16"/><path d="M4 16h16"/><path d="M10 4 8 20"/><path d="m16 4-2 16"/>"#);
const SELECT: &str =
    svg!(r#"<rect x="3" y="5" width="18" height="14" rx="3"/><path d="m9 11 3 3 3-3"/>"#);
const BUTTON: &str = svg!(r#"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3.5"/>"#);
const VACUUM: &str = svg!(
    r#"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3"/><path d="M12 3v3"/><path d="M6 17.5l2-2"/>"#
);
const WATER_HEATER: &str = svg!(
    r#"<path d="M12 3s6 6.2 6 11a6 6 0 0 1-12 0c0-4.8 6-11 6-11z"/><path d="M9.5 14.5a2.5 2.5 0 0 0 2.5 2.5"/>"#
);
const DOOR: &str = svg!(
    r#"<path d="M6 21V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v17"/><path d="M3 21h18"/><circle cx="15" cy="12" r="0.8"/>"#
);
const WINDOW: &str = svg!(
    r#"<rect x="4" y="3" width="16" height="18" rx="2"/><path d="M12 3v18"/><path d="M4 12h16"/>"#
);
const MOTION: &str = svg!(
    r#"<circle cx="13" cy="4.5" r="1.8"/><path d="m9 21 2.5-6-2.5-2.5 1.5-4.5 3.5 2 3 .5"/><path d="m11.5 15 3.5 2 .5 4"/><path d="m4 11 3-2"/>"#
);
const OCCUPANCY: &str = svg!(r#"<circle cx="12" cy="8" r="3.5"/><path d="M5 20a7 7 0 0 1 14 0"/>"#);
const SMOKE: &str = svg!(
    r#"<path d="M4 15c0-2 1.5-3 3-3 .3-2.5 2.2-4 4.5-4S15.7 9.5 16 12c2 0 4 1 4 3s-1.5 3-3.5 3h-9C5.5 18 4 17 4 15z"/>"#
);
const POWER: &str = svg!(r#"<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>"#);
const TEMPERATURE: &str = svg!(
    r#"<path d="M14 14.8V5a2 2 0 0 0-4 0v9.8a4 4 0 1 0 4 0z"/><path d="M12 9v7"/><path d="M17.5 6h3"/><path d="M17.5 10h3"/>"#
);
const HUMIDITY: &str = svg!(r#"<path d="M12 3s6 6.2 6 11a6 6 0 0 1-12 0c0-4.8 6-11 6-11z"/>"#);
const BATTERY: &str = svg!(
    r#"<rect x="2.5" y="7" width="17" height="10" rx="2.5"/><path d="M22 10.5v3"/><path d="M6 10.5v3"/><path d="M10 10.5v3"/>"#
);
const ILLUMINANCE: &str = svg!(
    r#"<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2.5"/><path d="M12 19v2.5"/><path d="m4.9 4.9 1.8 1.8"/><path d="m17.3 17.3 1.8 1.8"/><path d="M2.5 12H5"/><path d="M19 12h2.5"/><path d="m4.9 19.1 1.8-1.8"/><path d="m17.3 6.7 1.8-1.8"/>"#
);

/// Cover control glyphs: open (up), stop and close (down).
pub const CHEVRON_UP: &str = svg!(r#"<path d="m6 15 6-6 6 6"/>"#);
pub const STOP: &str = svg!(r#"<rect x="7" y="7" width="10" height="10" rx="2"/>"#);
pub const CHEVRON_DOWN: &str = svg!(r#"<path d="m6 9 6 6 6-6"/>"#);

const CHANNEL_ALARM: &str =
    svg!(r#"<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z"/><path d="M10 21h4"/>"#);
const CHANNEL_SECURITY: &str = svg!(r#"<path d="M12 3 5 6v5c0 4.5 3 8 7 10 4-2 7-5.5 7-10V6z"/>"#);
const CHANNEL_CAMERA: &str = svg!(
    r#"<path d="M4 8h3l1.5-2.5h7L17 8h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="3.5"/>"#
);
const CHANNEL_ENERGY: &str = svg!(r#"<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>"#);
const CHANNEL_CLIMATE: &str = svg!(
    r#"<path d="M14 14.8V5a2 2 0 0 0-4 0v9.8a4 4 0 1 0 4 0z"/><path d="M12 9v7"/><path d="M17.5 7h3"/>"#
);
const CHANNEL_SYSTEM: &str = svg!(
    r#"<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3"/><path d="M12 18.5v3"/><path d="M2.5 12h3"/><path d="M18.5 12h3"/><path d="m5.3 5.3 2.1 2.1"/><path d="m16.6 16.6 2.1 2.1"/><path d="m5.3 18.7 2.1-2.1"/><path d="m16.6 7.4 2.1-2.1"/>"#
);
const CHANNEL_MISC: &str = svg!(
    r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><circle cx="12" cy="7.8" r="0.8"/>"#
);

/// The glyph of an alert channel: bell, shield, camera, bolt, thermometer, gear, info circle.
pub fn channel_icon(channel: AlertChannel) -> &'static str {
    match channel {
        AlertChannel::Alarm => CHANNEL_ALARM,
        AlertChannel::Security => CHANNEL_SECURITY,
        AlertChannel::Camera => CHANNEL_CAMERA,
        AlertChannel::Energy => CHANNEL_ENERGY,
        AlertChannel::Climate => CHANNEL_CLIMATE,
        AlertChannel::System => CHANNEL_SYSTEM,
        AlertChannel::Misc => CHANNEL_MISC,
    }
}

/// The icon for an entity: the device class wins when we draw one, else the kind's.
pub fn kind_icon(kind: Kind, device_class: Option<&str>) -> &'static str {
    if let Some(icon) = device_class.and_then(device_class_icon) {
        return icon;
    }
    match kind {
        Kind::Light => LIGHT,
        Kind::Switch => SWITCH,
        Kind::Cover => COVER,
        Kind::Climate => CLIMATE,
        Kind::Sensor => SENSOR,
        Kind::BinarySensor => BINARY_SENSOR,
        Kind::AlarmPanel => ALARM,
        Kind::Number => NUMBER,
        Kind::Select => SELECT,
        Kind::Button => BUTTON,
        Kind::Vacuum => VACUUM,
        Kind::WaterHeater => WATER_HEATER,
    }
}

fn device_class_icon(device_class: &str) -> Option<&'static str> {
    Some(match device_class {
        "door" => DOOR,
        "window" => WINDOW,
        "motion" => MOTION,
        "occupancy" => OCCUPANCY,
        "smoke" => SMOKE,
        "power" | "energy" => POWER,
        "temperature" => TEMPERATURE,
        "humidity" => HUMIDITY,
        "battery" => BATTERY,
        "illuminance" => ILLUMINANCE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{channel_icon, kind_icon};
    use oikos_api::AlertChannel;
    use oikos_core::model::Kind;

    const KINDS: [Kind; 12] = [
        Kind::Light,
        Kind::Switch,
        Kind::Cover,
        Kind::Climate,
        Kind::Sensor,
        Kind::BinarySensor,
        Kind::AlarmPanel,
        Kind::Number,
        Kind::Select,
        Kind::Button,
        Kind::Vacuum,
        Kind::WaterHeater,
    ];

    #[test]
    fn every_kind_has_a_distinct_svg() {
        let icons: Vec<_> = KINDS.iter().map(|k| kind_icon(*k, None)).collect();
        for (i, icon) in icons.iter().enumerate() {
            assert!(icon.starts_with("<svg") && icon.ends_with("</svg>"));
            assert!(
                !icons[..i].contains(icon),
                "duplicate icon for {:?}",
                KINDS[i]
            );
        }
    }

    #[test]
    fn every_channel_has_a_distinct_svg() {
        let icons: Vec<_> = AlertChannel::ALL.iter().map(|c| channel_icon(*c)).collect();
        for (i, icon) in icons.iter().enumerate() {
            assert!(icon.starts_with("<svg") && icon.ends_with("</svg>"));
            assert!(
                !icons[..i].contains(icon),
                "duplicate icon for {:?}",
                AlertChannel::ALL[i]
            );
        }
    }

    #[test]
    fn device_class_overrides_and_unknown_falls_back() {
        let door = kind_icon(Kind::BinarySensor, Some("door"));
        assert_ne!(door, kind_icon(Kind::BinarySensor, None));
        assert_eq!(
            kind_icon(Kind::BinarySensor, Some("no-such-class")),
            kind_icon(Kind::BinarySensor, None)
        );
    }
}
