use lifx_core::{BuildOptions, Message, RawMessage, Service};
use std::collections::HashMap;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;

const PORT: u16 = 56700;

enum BulbColor {
    Unknown,
}

struct BulbInfo {
    address: SocketAddr,
    _color: BulbColor,
}

impl BulbInfo {
    fn new(address: SocketAddr) -> Self {
        Self {
            address,
            _color: BulbColor::Unknown,
        }
    }

    fn handle_message(&self, raw_message: RawMessage) -> Result<(), lifx_core::Error> {
        match Message::from_raw(&raw_message)? {
            Message::StateService { service, port } => {
                let port = port as u16;
                if service != Service::UDP || port != self.address.port() {
                    let address = SocketAddr::new(self.address.ip(), port);
                    println!("Unknown service on: {address}");
                }
            }
            Message::LightState {
                color,
                power,
                label,
                ..
            } => {
                todo!("color: {color:?}, power: {power}, label: {label}") // TODO
            }
            message => println!("Unhandled message: {message:?}"),
        }

        Ok(())
    }
}

pub struct BulbManager {
    /// Unique ID for use in replies
    source: u32,
    socket: UdpSocket,
    _bulbs: Arc<Mutex<HashMap<u64, BulbInfo>>>,
}

impl BulbManager {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let source = 0x1337C0D3;

        let socket = UdpSocket::bind("0.0.0.0:56700")?;
        socket.set_broadcast(true)?;

        let bulbs = Arc::new(Mutex::new(HashMap::new()));

        let manager = Self {
            source,
            socket: socket.try_clone()?,
            _bulbs: bulbs.clone(),
        };

        thread::spawn(move || Self::receive_messages(source, socket, bulbs));

        manager.discover()?;

        Ok(manager)
    }

    fn discover(&self) -> Result<(), Box<dyn Error>> {
        let message = RawMessage::build(
            &BuildOptions {
                source: self.source,
                ..Default::default()
            },
            Message::GetService,
        )?;

        let address = SocketAddr::from((Ipv4Addr::BROADCAST, PORT));

        self.socket.send_to(&message.pack()?, address)?;

        Ok(())
    }

    fn receive_messages(source: u32, socket: UdpSocket, bulbs: Arc<Mutex<HashMap<u64, BulbInfo>>>) {
        let mut buffer = [0; 1024]; // XXX

        loop {
            match socket.recv_from(&mut buffer) {
                Err(error) => panic!("Error receiving data: {error}"),
                Ok((num_bytes, src_addr)) => match RawMessage::unpack(&buffer[..num_bytes]) {
                    Err(error) => eprintln!("Error unpacking message: {error}"),
                    Ok(raw_message) => {
                        if raw_message.frame.source != source {
                            continue; // Skipping replies to others
                        }
                        if raw_message.frame_addr.target == 0 {
                            continue; // Skipping broadcasts
                        }

                        let mut bulbs = bulbs.lock().unwrap();
                        let bulb = bulbs
                            .entry(raw_message.frame_addr.target)
                            .and_modify(|bulb| bulb.address = src_addr)
                            .or_insert(BulbInfo::new(src_addr));

                        if let Err(error) = bulb.handle_message(raw_message) {
                            eprintln!("Error handling message: {error}");
                        }
                    }
                },
            }
        }
    }
}
