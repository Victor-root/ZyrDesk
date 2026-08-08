//! The first words exchanged on ZyrDesk's own channel.
//!
//! The client cannot open the local ports that stand in for the host's
//! engine until it knows their numbers. Those are not a convention: the
//! engine picks a base port when it starts and announces the real port
//! numbers to the other engine at every step of its protocol, so the
//! stand-ins have to carry exactly the same ones. The tunnel therefore
//! asks the tunnel, before any engine is involved.
//!
//! This channel is where the rest will go too: versions, pairing code,
//! clipboard, statistics. The greeting opens with a version number so
//! they can be added without leaving an older peer behind.

use std::io;

use tokio::io::AsyncWriteExt;
use zyr_proto::net::{BasePortOutOfRange, EnginePorts};
use zyr_transport::{Connection, SendStream};

use crate::channel::StreamChannel;
use crate::pump;

/// Version of this exchange.
const VERSION: u8 = 1;

/// Length on the wire: the version, then the base port.
const LENGTH: usize = 3;

/// What the host says to a client that has just arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Greeting {
    /// Engine ports of the host, all derived from its base port.
    pub engine: EnginePorts,
}

/// Asks the host what its engine runs on. Client side.
pub async fn ask(connection: &Connection) -> io::Result<Greeting> {
    let (mut sending, mut receiving) = connection.open_stream().await.map_err(io::Error::other)?;
    pump::announce(&mut sending, StreamChannel::ZyrDesk).await?;

    let mut answer = [0u8; LENGTH];
    receiving
        .read_exact(&mut answer)
        .await
        .map_err(io::Error::other)?;
    decode(answer)
}

/// Answers a client that has just arrived. Host side.
pub async fn answer(mut sending: SendStream, engine: EnginePorts) -> io::Result<()> {
    sending.write_all(&encode(Greeting { engine })).await?;
    sending.shutdown().await?;
    Ok(())
}

fn encode(greeting: Greeting) -> [u8; LENGTH] {
    let [high, low] = greeting.engine.base().to_be_bytes();
    [VERSION, high, low]
}

fn decode(bytes: [u8; LENGTH]) -> io::Result<Greeting> {
    if bytes[0] != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "l'autre ordinateur parle la version {} du tunnel, celui-ci la version {VERSION}",
                bytes[0]
            ),
        ));
    }
    let base = u16::from_be_bytes([bytes[1], bytes[2]]);
    let engine = EnginePorts::new(base).map_err(|e: BasePortOutOfRange| {
        io::Error::new(io::ErrorKind::InvalidData, e.to_string())
    })?;
    Ok(Greeting { engine })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports(base: u16) -> EnginePorts {
        EnginePorts::new(base).unwrap()
    }

    #[test]
    fn a_greeting_makes_the_round_trip() {
        for base in [42000, 42512, 42999] {
            let greeting = Greeting {
                engine: ports(base),
            };
            assert_eq!(decode(encode(greeting)).unwrap(), greeting);
        }
    }

    #[test]
    fn another_version_is_refused_rather_than_misread() {
        let mut bytes = encode(Greeting {
            engine: ports(42000),
        });
        bytes[0] = VERSION + 1;
        let failure = decode(bytes).unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_port_outside_the_engine_range_is_refused() {
        // A wrong base port would send the client's stand-in ports
        // anywhere: better to say so than to open them and wait.
        assert!(decode([VERSION, 0, 80]).is_err());
    }
}
