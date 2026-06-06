use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

#[derive(thiserror::Error, Debug)]
pub enum QuicheError {
    #[error("Quiche error: {0}")]
    Quiche(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("HTTP/3 error: {0}")]
    H3(String),
}

impl From<quiche::Error> for QuicheError {
    fn from(err: quiche::Error) -> Self {
        QuicheError::Quiche(format!("{:?}", err))
    }
}

pub struct H3Request {
    pub stream_id: u64,
    pub headers: Vec<quiche::h3::Header>,
    pub body: Vec<u8>,
    pub response_tx: mpsc::Sender<H3Response>,
}

pub struct H3Response {
    pub status: u16,
    pub headers: Vec<quiche::h3::Header>,
    pub body: Vec<u8>,
}

pub struct H3Server {
    addr: SocketAddr,
    cert_path: String,
    key_path: String,
}

impl H3Server {
    /// Creates a new HTTP/3 server instance.
    pub fn new(addr: SocketAddr, cert_path: String, key_path: String) -> Self {
        Self {
            addr,
            cert_path,
            key_path,
        }
    }

    /// Runs the HTTP/3 server loop.
    ///
    /// Algorithmic Complexity: $O(1)$ packet handling loop with $O(N)$ active connections.
    pub async fn run(&self) -> Result<(), QuicheError> {
        let socket = UdpSocket::bind(self.addr).await.map_err(|e| QuicheError::Io(e.to_string()))?;
        let socket = Arc::new(socket);

        let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION)?;
        config.set_application_proto_list(quiche::h3::APPLICATION_PROTOCOL)?;
        config.load_cert_chain_from_pem_file(&self.cert_path)?;
        config.load_priv_key_from_pem_file(&self.key_path)?;
        
        config.set_max_idle_timeout(5000);
        config.set_max_recv_udp_payload_size(1350);
        config.set_max_send_udp_payload_size(1350);
        config.set_initial_max_data(10_000_000);
        config.set_initial_max_stream_data_bidi_local(1_000_000);
        config.set_initial_max_stream_data_bidi_remote(1_000_000);
        config.set_initial_max_streams_bidi(100);
        config.set_initial_max_streams_uni(100);
        config.enable_early_data();

        let (request_tx, mut request_rx) = mpsc::channel::<H3Request>(100);

        // Run request processor in the background
        tokio::spawn(async move {
            while let Some(req) = request_rx.recv().await {
                // Return default 200 OK
                let resp = H3Response {
                    status: 200,
                    headers: vec![
                        quiche::h3::Header::new(b":status", b"200"),
                        quiche::h3::Header::new(b"content-type", b"text/plain"),
                    ],
                    body: b"Hello from Caffeine-Rust HTTP/3 Gateway!".to_vec(),
                };
                let _ = req.response_tx.send(resp).await;
            }
        });

        let mut buf = [0u8; 65535];
        let mut out = [0u8; 65535];
        let mut conns = HashMap::new();

        loop {
            tokio::select! {
                rec = socket.recv_from(&mut buf) => {
                    match rec {
                        Ok((len, from)) => {
                            let packet = &mut buf[..len];
                            let header = match quiche::Header::from_slice(packet, quiche::MAX_CONN_ID_LEN) {
                                Ok(h) => h,
                                Err(e) => {
                                    log::warn!("Failed to parse QUIC packet header: {:?}", e);
                                    continue;
                                }
                            };

                            let scid = header.scid.clone();
                            let dcid = header.dcid.clone();

                            let conn = conns.entry(dcid.clone()).or_insert_with(|| {
                                let local_addr = socket.local_addr().unwrap();
                                let c = quiche::accept(&scid, Some(&dcid), local_addr, from, &mut config).unwrap();
                                c
                            });

                            let recv_info = quiche::RecvInfo {
                                to: socket.local_addr().unwrap(),
                                from,
                            };

                            if let Err(e) = conn.recv(packet, recv_info) {
                                log::error!("QUIC recv failed: {:?}", e);
                                continue;
                            }

                            if conn.is_established() {
                                // Manage H3 stream
                                let mut h3_config = quiche::h3::Config::new().unwrap();
                                if let Ok(mut h3_conn) = quiche::h3::Connection::with_transport(conn, &mut h3_config) {
                                    let mut headers = Vec::new();
                                    loop {
                                        match h3_conn.poll(conn) {
                                            Ok((stream_id, quiche::h3::Event::Headers { list, .. })) => {
                                                headers = list;
                                                // Wait for finished request
                                                let (resp_tx, mut resp_rx) = mpsc::channel(1);
                                                let h3_req = H3Request {
                                                    stream_id,
                                                    headers: headers.clone(),
                                                    body: Vec::new(),
                                                    response_tx: resp_tx,
                                                };
                                                let _ = request_tx.send(h3_req).await;

                                                if let Some(resp) = resp_rx.recv().await {
                                                    let _ = h3_conn.send_response(conn, stream_id, &resp.headers, false);
                                                    let _ = h3_conn.send_body(conn, stream_id, &resp.body, true);
                                                }
                                            }
                                            Ok((_, quiche::h3::Event::Data)) => {
                                                // Handle body chunks if any
                                            }
                                            Ok((_, quiche::h3::Event::Finished)) => {
                                                break;
                                            }
                                            Err(quiche::h3::Error::Done) => {
                                                break;
                                            }
                                            Err(e) => {
                                                log::error!("H3 connection error: {:?}", e);
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            loop {
                                match conn.send(&mut out) {
                                    Ok((write_len, send_info)) => {
                                        if let Err(e) = socket.send_to(&out[..write_len], send_info.to).await {
                                            log::error!("UDP send failed: {:?}", e);
                                            break;
                                        }
                                    }
                                    Err(quiche::Error::Done) => {
                                        break;
                                    }
                                    Err(e) => {
                                        log::error!("QUIC send failed: {:?}", e);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("UDP receive error: {:?}", e);
                        }
                    }
                }
            }
        }
    }
}
