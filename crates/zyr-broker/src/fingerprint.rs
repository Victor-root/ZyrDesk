//! A fingerprint inside a JSON message: the same 64 hexadecimal
//! characters it is shown with everywhere else, so that a ticket read by
//! eye names the same machine as the window does.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zyr_transport::Fingerprint;

pub fn serialize<S: Serializer>(
    fingerprint: &Fingerprint,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    fingerprint.to_string().serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Fingerprint, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use zyr_transport::{Fingerprint, Identity};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Carrying {
        #[serde(with = "super")]
        who: Fingerprint,
    }

    #[test]
    fn a_fingerprint_travels_as_its_usual_spelling() {
        let who = Identity::generate().unwrap().fingerprint();
        let text = serde_json::to_string(&Carrying { who }).unwrap();
        assert!(text.contains(&format!("\"who\":\"{who}\"")), "{text}");
        let back: Carrying = serde_json::from_str(&text).unwrap();
        assert_eq!(back.who, who);
    }

    #[test]
    fn text_that_is_not_a_fingerprint_is_refused() {
        assert!(serde_json::from_str::<Carrying>(r#"{"who":"abc"}"#).is_err());
    }
}
