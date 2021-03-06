use mio::*;
use mio::net::*;
use mio::unix::UnixReady;
use pool::Checkout;
use std::io::ErrorKind;
use network::buffer_queue::BufferQueue;
use network::{ClientResult,Protocol,Readiness};
use network::protocol::ProtocolResult;
use rustls::{ServerSession, Session};

pub enum TlsState {
  Initial,
  Handshake,
  Established,
  Error,
}

pub struct TlsHandshake {
  pub stream:    TcpStream,
  pub session:   ServerSession,
  pub readiness: Readiness,
}

impl TlsHandshake {
  pub fn new(session: ServerSession, stream: TcpStream) -> TlsHandshake {
    TlsHandshake {
      stream:   stream,
      session:   session,
      readiness: Readiness {
        front_interest:  UnixReady::from(Ready::readable())
                           | UnixReady::hup() | UnixReady::error(),
        back_interest:   UnixReady::from(Ready::empty()),
        front_readiness: UnixReady::from(Ready::empty()),
        back_readiness:  UnixReady::from(Ready::empty()),
      },
    }
  }

  pub fn readable(&mut self) -> (ProtocolResult,ClientResult) {
    let mut can_read  = true;
    let mut can_write = true;

    loop {
      let mut can_work = false;

      if self.session.wants_read() && can_read {
        can_work = true;

        match self.session.read_tls(&mut self.stream) {
          Ok(_) => {},
          Err(e) => match e.kind() {
            ErrorKind::WouldBlock => can_read = false,
            _ => {
              error!("could not perform handshake: {:?}", e);
              return (ProtocolResult::Continue, ClientResult::CloseClient);
            }
          }
        }

        if let Err(e) = self.session.process_new_packets() {
          error!("could not perform handshake: {:?}", e);
          return (ProtocolResult::Continue, ClientResult::CloseClient);
        }
      }

      if self.session.wants_write() && can_write {
        can_work = true;

        match self.session.write_tls(&mut self.stream) {
          Ok(_) => {},
          Err(e) => match e.kind() {
            ErrorKind::WouldBlock => can_write = false,
            _ => {
              error!("could not perform handshake: {:?}", e);
              return (ProtocolResult::Continue, ClientResult::CloseClient);
            }
          }
        }

        if let Err(e) = self.session.process_new_packets() {
          error!("could not perform handshake: {:?}", e);
          return (ProtocolResult::Continue, ClientResult::CloseClient);
        }
      }

      if !can_work {
        break;
      }
    }

    if self.session.is_handshaking() {
      (ProtocolResult::Continue, ClientResult::Continue)
    } else {
      (ProtocolResult::Upgrade, ClientResult::Continue)
    }
  }
}

