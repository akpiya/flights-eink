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

static TCP_STATE: TcpClientState<1, 4096, 4096> = TcpClientState::new();

pub async fn get_airplanes<'a>(
    stack: Stack<'a>,
    ca: Certificate<'a>,
    tls:TlsReference<'a>
) -> Result<(), reqwless::Error> {
    let tcp = TcpClient::new(stack, &TCP_STATE);
    let dns = DnsSocket::new(stack);
    let tls_config = TlsConfig::new(TlsVersion::Tls1_3, ca, None, tls);
    let mut client = HttpClient::new_with_tls(&tcp, &dns, tls_config);
    let mut response_buffer = [0; 1024];

    let response = client
        .request(Method::GET, "URL")
        .await?
        .send(&mut response_buffer)
        .await?;

    Ok(())
}

