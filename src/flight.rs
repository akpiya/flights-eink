use core::fmt::Write;

use embassy_net::{
    Stack,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use heapless::{String, Vec};
use reqwless::{
    Certificate, TlsReference, TlsVersion,
    client::{HttpClient, TlsConfig},
    request::Method,
};
use serde::Deserialize;
use static_cell::ConstStaticCell;

// More aircraft than this fails the whole parse.
const MAX_FLIGHTS: usize = 32;
// adsb.lol sends at most ~550 bytes per aircraft; the rest leaves
// room for the HTTP headers sharing the buffer. The whole body must fit.
const BYTES_PER_FLIGHT: usize = 640;
const RESPONSE_BYTES: usize = MAX_FLIGHTS * BYTES_PER_FLIGHT;

// ConstStaticCell keeps these large buffers out of the stack, which `StaticCell::init`
// would briefly put them on.
static TCP_STATE: ConstStaticCell<TcpClientState<1, 4096, 4096>> =
    ConstStaticCell::new(TcpClientState::new());
static RESPONSE: ConstStaticCell<[u8; RESPONSE_BYTES]> = ConstStaticCell::new([0; RESPONSE_BYTES]);

#[derive(Deserialize)]
pub struct Flight {
    // Missing for aircraft that haven't reported a position recently.
    pub lat: Option<f32>,
    pub lon: Option<f32>,
    // Padded with trailing spaces, and missing for some aircraft.
    pub flight: Option<String<8>>,
    pub gs: Option<f32>,
    pub track: Option<f32>,
}

#[derive(Debug, defmt::Format)]
pub enum FlightError {
    Http(reqwless::Error),
    Status(u16),
    Json,
}

pub struct FlightClient<'a> {
    tcp: TcpClient<'a, 1, 4096, 4096>,
    dns: DnsSocket<'a>,
    response: &'static mut [u8; RESPONSE_BYTES],
    ca: Certificate<'a>,
    tls: TlsReference<'a>,
}

impl<'a> FlightClient<'a> {
    // Only call once: the buffers are statics.
    pub fn new(stack: Stack<'a>, ca: Certificate<'a>, tls: TlsReference<'a>) -> Self {
        Self {
            tcp: TcpClient::new(stack, TCP_STATE.take()),
            dns: DnsSocket::new(stack),
            response: RESPONSE.take(),
            ca,
            tls,
        }
    }

    /// Aircraft within `radius_nm` nautical miles (the API's unit, max 250) of a point,
    /// keeping only those with a position. The API allows one request per second.
    pub async fn get_flights(
        &mut self,
        latitude: f32,
        longitude: f32,
        radius_nm: u32,
    ) -> Result<Vec<Flight, MAX_FLIGHTS>, FlightError> {
        let tls_config = TlsConfig::new(TlsVersion::Tls1_3, self.ca.clone(), None, self.tls);
        let mut client = HttpClient::new_with_tls(&self.tcp, &self.dns, tls_config);
        let mut url = String::<128>::new();
        write!(url, "https://api.adsb.lol/v2/point/{latitude}/{longitude}/{radius_nm}")
            .expect("URL fits in 128 bytes");

        let mut request = client
            .request(Method::GET, &url)
            .await
            .map_err(FlightError::Http)?;
        let response = request
            .send(self.response)
            .await
            .map_err(FlightError::Http)?;
        if !response.status.is_successful() {
            return Err(FlightError::Status(response.status.0));
        }
        let body = response.body().read_to_end().await.map_err(FlightError::Http)?;

        #[derive(Deserialize)]
        struct Response {
            ac: Vec<Flight, MAX_FLIGHTS>,
        }
        let (mut response, _) =
            serde_json_core::from_slice::<Response>(body).map_err(|_| FlightError::Json)?;
        response.ac.retain(|flight| flight.lat.is_some() && flight.lon.is_some());
        Ok(response.ac)
    }
}
