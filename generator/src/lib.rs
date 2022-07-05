use rand::prelude::*;
use serde_derive::Serialize;

const TWITCH_NAMES: &[&str] = &[
    "TobogganBoi",
    "Gachi_Mike",
    "Garen200iq",
    "DaBoiii",
    "MichauZBolzgi",
    "ByleLajkoniK",
];

const TWITCH_MESSAGES: &[&str] = &[
    "LUL LUL :Kappa::Kappa:",
    "NICE :4Head:",
    "GIB BOOBAZ :GachiBald::GachiBald:",
    ":KEKW::KEKW::KEKW:",
    "REKTTT",
    "HUH",
    ":POGGERS::POGGERS:",
    "ENH :OMEGALUL:",
    "ASS savage :POGGERS:",
    "TRUE :gachiGASM:",
    "PRIDE :KappaPride:",
    "TBC :OMEGALUL:",
    "?????????????",
    "RETAIL :OMEGALUL:",
    "HOLY",
    "hahahahaha",
    ":PepeLaugh",
    "JESUS :LUL:",
    "DO IT :POGGERS:",
    ":monkaW:",
    ":Sadge::Sadge::PJSalt:",
    ":AYAYA::AYAYA::AYAYA:",
];

pub fn generate_message() -> TwitchMessage {
    let mut rng = thread_rng();

    let message = TWITCH_MESSAGES
        .choose(&mut rng)
        .expect("non-empty twitch messages slice");
    let author = TWITCH_NAMES
        .choose(&mut rng)
        .expect("non-empty twitch messages slice");

    TwitchMessage::Static {
        author: *author,
        message: *message,
    }
}

#[derive(Serialize, Clone)]
#[serde(untagged)]
pub enum TwitchMessage {
    Owned {
        author: &'static str,
        message: String,
    },
    Static {
        author: &'static str,
        message: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use crate::generate_message;
    use crate::TwitchMessage;
    use crate::TWITCH_MESSAGES;
    use crate::TWITCH_NAMES;

    #[test]
    fn it_works() {
        let generated = generate_message();

        match generated {
            TwitchMessage::Static { author, message } => {
                assert!(TWITCH_NAMES.contains(&author));
                assert!(TWITCH_MESSAGES.contains(&message));
            }
            _ => {
                panic!("generate_message returned non-static variant of TwitchMessage.");
            }
        }
    }
}
