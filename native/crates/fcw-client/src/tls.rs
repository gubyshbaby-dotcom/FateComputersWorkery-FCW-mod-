//! Проверка сертификата демона по закреплённому SHA-256 (самоподписанный).

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Debug)]
pub struct PinnedVerifier {
    pin: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl PinnedVerifier {
    pub fn new(pin: [u8; 32], provider: Arc<CryptoProvider>) -> Self {
        PinnedVerifier { pin, provider }
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let got = Sha256::digest(end_entity.as_ref());
        if got.as_slice() == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch: got {}, expected {}",
                hex::encode(got),
                hex::encode(self.pin)
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// hex -> 32 байта.
pub fn parse_pin(hex_str: &str) -> Result<[u8; 32], String> {
    let clean: String = hex_str.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let v = hex::decode(&clean).map_err(|e| format!("bad certificate fingerprint: {e}"))?;
    v.try_into().map_err(|_| "certificate fingerprint must be 32 bytes (64 hex digits)".to_string())
}

pub fn client_config(pin: [u8; 32]) -> Result<quinn::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier::new(pin, provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![fcw_proto::ALPN.to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(|e| e.to_string())?;
    let mut cfg = quinn::ClientConfig::new(Arc::new(crypto));
    let mut t = quinn::TransportConfig::default();
    t.max_concurrent_uni_streams(256u32.into())
        .max_concurrent_bidi_streams(4u32.into())
        .keep_alive_interval(Some(std::time::Duration::from_secs(5)))
        .max_idle_timeout(Some(std::time::Duration::from_secs(30).try_into().map_err(|_| "idle timeout")?))
        .stream_receive_window((16u32 * 1024 * 1024).into())
        .receive_window((64u32 * 1024 * 1024).into())
        .datagram_receive_buffer_size(Some(512 * 1024));
    cfg.transport_config(Arc::new(t));
    Ok(cfg)
}
