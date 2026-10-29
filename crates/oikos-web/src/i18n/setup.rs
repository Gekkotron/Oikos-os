//! FR/EN text: the setup wizard.

use super::*;

/// Fixed words of the setup wizard. Later steps add their own entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupText {
    /// The overlay shown while the hub restarts to apply a save.
    Applying,
    /// The overlay's note when the hub did not answer again in time.
    ApplyFailed,
    /// The overlay's button that waits for the hub again.
    Retry,
    /// The accessible name of the wizard's progress line.
    Progress,
    StepWelcome,
    StepMqtt,
    StepDatabase,
    StepDevices,
    StepServices,
    StepDone,
    /// The Welcome step's title.
    WelcomeTitle,
    WelcomeLead,
    Language,
    LangEnglish,
    LangFrench,
    /// The location form's heading.
    LocationTitle,
    City,
    Country,
    Search,
    NoPlaceFound,
    /// The disclosure that holds latitude, longitude and time zone.
    CorrectCoords,
    Latitude,
    Longitude,
    TimeZone,
    LocationUse,
    /// The first of the location form's problem messages, shown under the form.
    CityRequired,
    CountryTwoLetters,
    LatitudeRange,
    LongitudeRange,
    TimeZoneUnknown,
    Next,
    Save,
    Back,
    /// The MQTT step's title and lead line.
    MqttTitle,
    MqttLead,
    DatabaseTitle,
    DatabaseLead,
    TestConnection,
    Testing,
    /// The disclosure that holds the rarely changed fields.
    Advanced,
    Host,
    Port,
    User,
    Password,
    ClientId,
    Z2mBase,
    DatabaseName,
    /// Placeholder of a password field whose value is stored; the field stays empty to keep it.
    SecretSaved,
    /// "This database already holds Oikos history since {date}".
    HistorySince,
    PortInvalid,
    FieldRequired,
    /// The Database step's note: it has no Skip.
    NoSkipDatabase,
    /// The Settings pages' note when the stored values could not be read.
    LoadFailed,
}

pub fn setup_text(lang: Lang, k: SetupText) -> &'static str {
    let (en, fr) = match k {
        SetupText::Applying => ("Applying…", "Application…"),
        SetupText::ApplyFailed => (
            "The hub did not come back. Check its log.",
            "Le hub n'est pas revenu. Consultez son journal.",
        ),
        SetupText::Retry => ("Retry", "Réessayer"),
        SetupText::Progress => ("Setup progress", "Progression de la configuration"),
        SetupText::StepWelcome => ("Welcome", "Bienvenue"),
        SetupText::StepMqtt => ("MQTT broker", "Broker MQTT"),
        SetupText::StepDatabase => ("Database", "Base de données"),
        SetupText::StepDevices => ("Devices", "Appareils"),
        SetupText::StepServices => ("Services", "Services"),
        SetupText::StepDone => ("Done", "Terminé"),
        SetupText::WelcomeTitle => ("Welcome to Oikos", "Bienvenue dans Oikos"),
        SetupText::WelcomeLead => (
            "A few questions and your home is ready. Everything can be changed later in Settings.",
            "Quelques questions et votre maison est prête. Tout peut être modifié plus tard dans les Réglages.",
        ),
        SetupText::Language => ("Language", "Langue"),
        SetupText::LangEnglish => ("English", "English"),
        SetupText::LangFrench => ("Français", "Français"),
        SetupText::LocationTitle => ("Where is your home?", "Où est votre maison ?"),
        SetupText::City => ("City", "Ville"),
        SetupText::Country => ("Country", "Pays"),
        SetupText::Search => ("Search", "Rechercher"),
        SetupText::NoPlaceFound => (
            "No place found. Enter the coordinates by hand.",
            "Aucun lieu trouvé. Saisissez les coordonnées à la main.",
        ),
        SetupText::CorrectCoords => ("Coordinates and time zone", "Coordonnées et fuseau horaire"),
        SetupText::Latitude => ("Latitude", "Latitude"),
        SetupText::Longitude => ("Longitude", "Longitude"),
        SetupText::TimeZone => ("Time zone", "Fuseau horaire"),
        SetupText::LocationUse => (
            "Used for sunrise and sunset in flows, and for the weather.",
            "Sert au lever et au coucher du soleil dans les flux, et à la météo.",
        ),
        SetupText::CityRequired => ("Enter a city.", "Saisissez une ville."),
        SetupText::CountryTwoLetters => (
            "The country is a two-letter code, such as FR.",
            "Le pays est un code à deux lettres, comme FR.",
        ),
        SetupText::LatitudeRange => (
            "Latitude is between -90 and 90.",
            "La latitude est comprise entre -90 et 90.",
        ),
        SetupText::LongitudeRange => (
            "Longitude is between -180 and 180.",
            "La longitude est comprise entre -180 et 180.",
        ),
        SetupText::TimeZoneUnknown => (
            "Unknown time zone. Use a name such as Europe/Paris.",
            "Fuseau horaire inconnu. Utilisez un nom comme Europe/Paris.",
        ),
        SetupText::Next => ("Next", "Suivant"),
        SetupText::Save => ("Save", "Enregistrer"),
        SetupText::Back => ("Back", "Retour"),
        SetupText::MqttTitle => ("Connect to your MQTT broker", "Connectez votre broker MQTT"),
        SetupText::MqttLead => (
            "Oikos talks to your devices through MQTT. Filled in for the broker that came with Oikos.",
            "Oikos parle à vos appareils via MQTT. Pré-rempli pour le broker fourni avec Oikos.",
        ),
        SetupText::DatabaseTitle => ("Connect to the database", "Connectez la base de données"),
        SetupText::DatabaseLead => (
            "TimescaleDB keeps your history and settings. Filled in for the database that came with Oikos.",
            "TimescaleDB conserve votre historique et vos réglages. Pré-rempli pour la base fournie avec Oikos.",
        ),
        SetupText::TestConnection => ("Test connection", "Tester la connexion"),
        SetupText::Testing => ("Testing…", "Test en cours…"),
        SetupText::Advanced => ("Advanced", "Avancé"),
        SetupText::Host => ("Host", "Hôte"),
        SetupText::Port => ("Port", "Port"),
        SetupText::User => ("User", "Utilisateur"),
        SetupText::Password => ("Password", "Mot de passe"),
        SetupText::ClientId => ("Client id", "Identifiant client"),
        SetupText::Z2mBase => ("Zigbee2MQTT base topic", "Topic de base Zigbee2MQTT"),
        SetupText::DatabaseName => ("Database", "Base de données"),
        SetupText::SecretSaved => (
            "Saved, enter a new one to replace it",
            "Enregistré, saisissez-en un nouveau pour le remplacer",
        ),
        SetupText::HistorySince => (
            "This database already holds Oikos history since {date}.",
            "Cette base contient déjà l'historique d'Oikos depuis le {date}.",
        ),
        SetupText::PortInvalid => (
            "The port is a number from 1 to 65535.",
            "Le port est un nombre de 1 à 65535.",
        ),
        SetupText::FieldRequired => (
            "Fill in the host, the database and the user.",
            "Renseignez l'hôte, la base et l'utilisateur.",
        ),
        SetupText::NoSkipDatabase => (
            "No Skip here: the database is required.",
            "Pas de Passer ici : la base de données est obligatoire.",
        ),
        SetupText::LoadFailed => (
            "Could not read the current values:",
            "Impossible de lire les valeurs actuelles :",
        ),
    };
    match lang {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// A translator for [`SetupText`] bound to the language in context (reactive).
pub fn use_setup_text() -> impl Fn(SetupText) -> &'static str + Copy + 'static {
    let lang = use_lang();
    move |k| setup_text(lang(), k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_translated() {
        assert_table(
            &[
                SetupText::Applying,
                SetupText::ApplyFailed,
                SetupText::Retry,
                SetupText::Progress,
                SetupText::StepWelcome,
                SetupText::StepMqtt,
                SetupText::StepDatabase,
                SetupText::StepDevices,
                SetupText::StepServices,
                SetupText::StepDone,
                SetupText::WelcomeTitle,
                SetupText::WelcomeLead,
                SetupText::Language,
                SetupText::LangEnglish,
                SetupText::LangFrench,
                SetupText::LocationTitle,
                SetupText::City,
                SetupText::Country,
                SetupText::Search,
                SetupText::NoPlaceFound,
                SetupText::CorrectCoords,
                SetupText::Latitude,
                SetupText::Longitude,
                SetupText::TimeZone,
                SetupText::LocationUse,
                SetupText::CityRequired,
                SetupText::CountryTwoLetters,
                SetupText::LatitudeRange,
                SetupText::LongitudeRange,
                SetupText::TimeZoneUnknown,
                SetupText::Next,
                SetupText::Save,
                SetupText::Back,
                SetupText::MqttTitle,
                SetupText::MqttLead,
                SetupText::DatabaseTitle,
                SetupText::DatabaseLead,
                SetupText::TestConnection,
                SetupText::Testing,
                SetupText::Advanced,
                SetupText::Host,
                SetupText::Port,
                SetupText::User,
                SetupText::Password,
                SetupText::ClientId,
                SetupText::Z2mBase,
                SetupText::DatabaseName,
                SetupText::SecretSaved,
                SetupText::HistorySince,
                SetupText::PortInvalid,
                SetupText::FieldRequired,
                SetupText::NoSkipDatabase,
                SetupText::LoadFailed,
            ],
            setup_text,
            &[
                SetupText::StepServices,
                SetupText::LangEnglish,
                SetupText::LangFrench,
                SetupText::Latitude,
                SetupText::Longitude,
                SetupText::Port,
                SetupText::Z2mBase,
            ],
        );
    }
}
