use embassy_net::{
    Stack,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    Certificate, TlsReference, TlsVersion,
    client::{HttpClient, TlsConfig},
};
use static_cell::ConstStaticCell;

use crate::flight::RESPONSE_BYTES;

type Tcp<'a> = TcpClient<'a, 1, 4096, 4096>;

// ConstStaticCell keeps these large buffers out of the stack, which `StaticCell::init`
// would briefly put them on.
static TCP_STATE: ConstStaticCell<TcpClientState<1, 4096, 4096>> =
    ConstStaticCell::new(TcpClientState::new());
static BUFFER: ConstStaticCell<[u8; RESPONSE_BYTES]> = ConstStaticCell::new([0; RESPONSE_BYTES]);

/// Everything an HTTPS request needs, shared by the flight and update requests so
/// they reuse one set of buffers. Requests run one at a time.
pub struct Https<'a> {
    tcp: Tcp<'a>,
    dns: DnsSocket<'a>,
    ca: Certificate<'a>,
    tls: TlsReference<'a>,
    buffer: &'static mut [u8; RESPONSE_BYTES],
}

impl<'a> Https<'a> {
    // Only call once: the buffers are statics.
    pub fn new(stack: Stack<'a>, ca: Certificate<'a>, tls: TlsReference<'a>) -> Self {
        Self {
            tcp: TcpClient::new(stack, TCP_STATE.take()),
            dns: DnsSocket::new(stack),
            ca,
            tls,
            buffer: BUFFER.take(),
        }
    }

    /// A client for one request, plus the buffer to receive the response into.
    pub fn client(&mut self) -> (HttpClient<'_, Tcp<'a>, DnsSocket<'a>>, &mut [u8]) {
        let tls = TlsConfig::new(TlsVersion::Tls1_3, self.ca.clone(), None, self.tls);
        (
            HttpClient::new_with_tls(&self.tcp, &self.dns, tls),
            &mut self.buffer[..],
        )
    }
}
