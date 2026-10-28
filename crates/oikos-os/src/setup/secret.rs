//! How a `PUT` treats a secret: absent keeps, `null` clears, a string sets.

use oikos_api::Secret;

/// A new address (`host:port`) needs the secret again: otherwise anyone on the LAN could point a service at
/// their own machine and collect the stored password.
pub fn require_for_new_address(
    old: Option<&str>,
    new: &str,
    secret: &Secret,
) -> Result<(), &'static str> {
    let moved = old.is_some_and(|old| !old.eq_ignore_ascii_case(new));
    if moved && *secret == Secret::Keep {
        return Err("Enter the password for the new address");
    }
    Ok(())
}

/// The secret after the input is applied to what is stored.
pub fn apply(stored: Option<String>, input: Secret) -> Option<String> {
    match input {
        Secret::Keep => stored,
        Secret::Clear => None,
        Secret::Set(s) => Some(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_api::MqttInput;

    fn input(body: &str) -> Secret {
        let json = format!(r#"{{"host":"h","port":1{body}}}"#);
        serde_json::from_str::<MqttInput>(&json).unwrap().password
    }

    #[test]
    fn absent_is_keep() {
        assert_eq!(input(""), Secret::Keep);
        assert_eq!(
            apply(Some("old".into()), Secret::Keep).as_deref(),
            Some("old")
        );
    }

    #[test]
    fn null_is_clear() {
        assert_eq!(input(r#","password":null"#), Secret::Clear);
        assert_eq!(apply(Some("old".into()), Secret::Clear), None);
    }

    #[test]
    fn string_is_set() {
        assert_eq!(input(r#","password":"new""#), Secret::Set("new".into()));
        assert_eq!(
            apply(Some("old".into()), Secret::Set("new".into())).as_deref(),
            Some("new")
        );
    }

    #[test]
    fn new_host_without_password_is_refused() {
        let err = require_for_new_address(Some("a.example.com"), "b.example.com", &Secret::Keep);
        assert_eq!(err, Err("Enter the password for the new address"));
        assert!(require_for_new_address(Some("a"), "b", &Secret::Set("p".into())).is_ok());
        assert!(require_for_new_address(Some("a"), "b", &Secret::Clear).is_ok());
    }

    #[test]
    fn same_host_other_port_is_a_new_address() {
        let err = require_for_new_address(Some("h:1883"), "h:1884", &Secret::Keep);
        assert!(err.is_err());
    }

    #[test]
    fn same_host_keeps_password() {
        assert!(require_for_new_address(Some("a"), "a", &Secret::Keep).is_ok());
        assert!(require_for_new_address(None, "b", &Secret::Keep).is_ok());
    }
}
