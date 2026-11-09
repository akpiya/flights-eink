use embassy_net::{
    Stack,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    client::{HttpClient, TlsConfig},
    request::Method,
    Certificate, TlsReference, TlsVersion, X509,
};

use static_cell::StaticCell;

static TCP_STATE: StaticCell<TcpClientState<1, 4096, 4096>> = StaticCell::new();

pub struct Flight {
    latitude: f32,
    longitude: f32,
    call_sign: [u8; 8],
    call_sign_len: u8,
    ground_speed: u32, // knots
    heading: u32,      // degrees
    altitude: u32,     // feet
}

pub async fn get_flights<'a>(
    stack: Stack<'a>,
    ca: Certificate<'a>,
    tls:TlsReference<'a>,
    latitude: f32,
    longitude: f32,
    radius: u32,
) -> Result<(), reqwless::Error> {
    let tcp = TcpClient::new(stack, TCP_STATE.init(TcpClientState::new()));
    let dns = DnsSocket::new(stack);
    let tls_config = TlsConfig::new(TlsVersion::Tls1_3, ca, None, tls);
    let mut client = HttpClient::new_with_tls(&tcp, &dns, tls_config);
    let mut response_buffer = [0; 1024];

    let response = client
        .request(Method::GET, "URL")
        .await?
        .send(&mut response_buffer)
        .await?;

    for

    Ok(())
}
